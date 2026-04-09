import hashlib
import os
import time
import unittest
from unittest.mock import MagicMock, patch

os.environ["TABLE_NAME"] = "test-table"

# Prevent module-level boto3 import from connecting
with patch.dict("os.environ", {"AWS_DEFAULT_REGION": "us-east-1"}):
    import app


def _make_share(share_id="abc123", password="secret", expired=False, attempts=0):
    salt = os.urandom(32)
    pw_hash = hashlib.pbkdf2_hmac("sha256", password.encode(), salt, 600_000).hex()
    ttl = int(time.time()) + (-1 if expired else 3600)
    return {
        "share_id": share_id,
        "password_hash": pw_hash,
        "salt": salt.hex(),
        "bucket_name": "test-bucket",
        "object_key": "prefix/file.zip",
        "presigned_time": 3600,
        "expiry_ttl": ttl,
        "attempts": attempts,
        "max_attempts": 5,
        "region": "us-east-1",
    }


def _get_event(share_id, method="GET", body=None):
    ev = {
        "requestContext": {"http": {"method": method}},
        "pathParameters": {"share_id": share_id},
    }
    if body:
        ev["body"] = body
        ev["isBase64Encoded"] = False
    return ev


class TestGetHandler(unittest.TestCase):
    @patch.object(app, "_ddb")
    def test_valid_share_returns_form(self, mock_ddb):
        item = _make_share()
        mock_ddb.return_value.get_item.return_value = {"Item": item}
        resp = app.handler(_get_event("abc123", "GET"), None)
        self.assertEqual(resp["statusCode"], 200)
        self.assertIn("password", resp["body"])
        self.assertIn("Unlock", resp["body"])

    @patch.object(app, "_ddb")
    def test_missing_share_returns_404(self, mock_ddb):
        mock_ddb.return_value.get_item.return_value = {}
        resp = app.handler(_get_event("missing", "GET"), None)
        self.assertEqual(resp["statusCode"], 404)

    @patch.object(app, "_ddb")
    def test_expired_share_returns_410(self, mock_ddb):
        item = _make_share(expired=True)
        mock_ddb.return_value.get_item.return_value = {"Item": item}
        resp = app.handler(_get_event("abc123", "GET"), None)
        self.assertEqual(resp["statusCode"], 410)
        self.assertIn("expired", resp["body"])

    @patch.object(app, "_ddb")
    def test_locked_share_returns_410(self, mock_ddb):
        item = _make_share(attempts=5)
        mock_ddb.return_value.get_item.return_value = {"Item": item}
        resp = app.handler(_get_event("abc123", "GET"), None)
        self.assertEqual(resp["statusCode"], 410)
        self.assertIn("locked", resp["body"])


class TestPostHandler(unittest.TestCase):
    @patch.object(app, "_s3_client")
    @patch.object(app, "_ddb")
    def test_correct_password_returns_url(self, mock_ddb, mock_s3):
        item = _make_share(password="test123")
        mock_ddb.return_value.get_item.return_value = {"Item": item}
        mock_s3.return_value.generate_presigned_url.return_value = "https://s3.example.com/file"

        resp = app.handler(_get_event("abc123", "POST", body="password=test123"), None)
        self.assertEqual(resp["statusCode"], 200)
        self.assertIn("https://s3.example.com/file", resp["body"])

    @patch.object(app, "_ddb")
    def test_wrong_password_increments_attempts(self, mock_ddb):
        item = _make_share(password="correct")
        mock_ddb.return_value.get_item.return_value = {"Item": item}

        resp = app.handler(_get_event("abc123", "POST", body="password=wrong"), None)
        self.assertEqual(resp["statusCode"], 200)
        self.assertIn("Wrong password", resp["body"])
        self.assertIn("4 attempts", resp["body"])
        mock_ddb.return_value.update_item.assert_called_once()

    @patch.object(app, "_ddb")
    def test_fifth_wrong_attempt_locks(self, mock_ddb):
        item = _make_share(password="correct", attempts=4)
        mock_ddb.return_value.get_item.return_value = {"Item": item}

        resp = app.handler(_get_event("abc123", "POST", body="password=wrong"), None)
        self.assertEqual(resp["statusCode"], 410)
        self.assertIn("locked", resp["body"])

    @patch.object(app, "_ddb")
    def test_locked_share_rejects_correct_password(self, mock_ddb):
        item = _make_share(password="correct", attempts=5)
        mock_ddb.return_value.get_item.return_value = {"Item": item}

        resp = app.handler(_get_event("abc123", "POST", body="password=correct"), None)
        self.assertEqual(resp["statusCode"], 410)


if __name__ == "__main__":
    unittest.main()
