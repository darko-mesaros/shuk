import os
import time
import hashlib
import urllib.parse
import boto3
from botocore.config import Config as BotoConfig

TABLE_NAME = os.environ.get("TABLE_NAME", "shuk-password-shares")

# Lazy-initialized at first invocation
_ddb_table = None
_s3_clients = {}


def _ddb():
    global _ddb_table
    if _ddb_table is None:
        _ddb_table = boto3.resource("dynamodb").Table(TABLE_NAME)
    return _ddb_table


def _s3_client(region):
    if region not in _s3_clients:
        _s3_clients[region] = boto3.client("s3", region_name=region, config=BotoConfig(signature_version="s3v4"))
    return _s3_clients[region]


def handler(event, context):
    method = event.get("requestContext", {}).get("http", {}).get("method", "GET")
    share_id = event.get("pathParameters", {}).get("share_id")

    if not share_id:
        return _html_response(400, "Bad request")

    if method == "GET":
        return handle_get(share_id)
    elif method == "POST":
        return handle_post(share_id, event)
    return _html_response(405, "Method not allowed")


def handle_get(share_id):
    item = _get_share(share_id)
    if not item:
        return _html_response(404, "Share not found", error=True)

    err = _check_validity(item)
    if err:
        return _html_response(410, err, error=True)

    return _html_response(200, _form_html(share_id))


def handle_post(share_id, event):
    item = _get_share(share_id)
    if not item:
        return _html_response(404, "Share not found", error=True)

    err = _check_validity(item)
    if err:
        return _html_response(410, err, error=True)

    body = event.get("body", "")
    if event.get("isBase64Encoded"):
        import base64
        body = base64.b64decode(body).decode("utf-8")

    params = urllib.parse.parse_qs(body)
    password = params.get("password", [""])[0]

    if not password:
        return _html_response(200, _form_html(share_id, error="Please enter a password"))

    # Hash and compare
    salt = bytes.fromhex(item["salt"])
    expected = item["password_hash"]
    provided_hash = hashlib.pbkdf2_hmac("sha256", password.encode(), salt, 600_000).hex()

    if provided_hash == expected:
        # Generate presigned download URL
        url = _s3_client(item["region"]).generate_presigned_url(
            "get_object",
            Params={"Bucket": item["bucket_name"], "Key": item["object_key"]},
            ExpiresIn=int(item["presigned_time"]),
        )
        return _html_response(200, _success_html(url))

    # Wrong password — increment attempts
    attempts = int(item.get("attempts", 0)) + 1
    max_attempts = int(item.get("max_attempts", 5))
    _ddb().update_item(
        Key={"share_id": share_id},
        UpdateExpression="SET attempts = :a",
        ExpressionAttributeValues={":a": attempts},
    )

    if attempts >= max_attempts:
        return _html_response(410, "This link has been locked after too many failed attempts.", error=True)

    remaining = max_attempts - attempts
    return _html_response(200, _form_html(share_id, error=f"Wrong password. {remaining} attempt{'s' if remaining != 1 else ''} remaining."))


# --- helpers ---

def _get_share(share_id):
    resp = _ddb().get_item(Key={"share_id": share_id})
    return resp.get("Item")


def _check_validity(item):
    now = int(time.time())
    if now > int(item.get("expiry_ttl", 0)):
        return "This link has expired."
    if int(item.get("attempts", 0)) >= int(item.get("max_attempts", 5)):
        return "This link has been locked after too many failed attempts."
    return None


def _html_response(status, body, error=False):
    if error:
        body = _error_html(body)
    return {
        "statusCode": status,
        "headers": {"Content-Type": "text/html"},
        "body": body,
    }


def _page(title, content):
    return f"""<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title} — shuk</title>
<style>
*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:system-ui,sans-serif;background:#0f172a;color:#e2e8f0;min-height:100vh;display:flex;align-items:center;justify-content:center}}
.card{{background:#1e293b;border-radius:12px;padding:2rem;max-width:420px;width:90%;box-shadow:0 4px 24px rgba(0,0,0,.4)}}
h1{{font-size:1.4rem;margin-bottom:.5rem}}
p{{color:#94a3b8;margin-bottom:1.2rem;font-size:.95rem}}
.error{{color:#f87171;font-size:.9rem;margin-bottom:1rem}}
input[type=password]{{width:100%;padding:.7rem;border-radius:8px;border:1px solid #334155;background:#0f172a;color:#e2e8f0;font-size:1rem;margin-bottom:1rem}}
input[type=password]:focus{{outline:none;border-color:#3b82f6}}
button{{width:100%;padding:.7rem;border-radius:8px;border:none;background:#3b82f6;color:#fff;font-size:1rem;cursor:pointer;font-weight:600}}
button:hover{{background:#2563eb}}
a.dl{{display:inline-block;margin-top:1rem;padding:.7rem 1.5rem;border-radius:8px;background:#22c55e;color:#fff;text-decoration:none;font-weight:600}}
a.dl:hover{{background:#16a34a}}
.emoji{{font-size:2rem;margin-bottom:.8rem}}
</style></head><body><div class="card">{content}</div></body></html>"""


def _form_html(share_id, error=None):
    err_block = f'<div class="error">{error}</div>' if error else ""
    return _page("Enter password", f"""
<div class="emoji">🔒</div>
<h1>Password required</h1>
<p>This file is password-protected. Enter the password to download.</p>
{err_block}
<form method="POST" action="/share/{share_id}">
<input type="password" name="password" placeholder="Password" autofocus required>
<button type="submit">Unlock &amp; Download</button>
</form>""")


def _success_html(url):
    return _page("Ready to download", f"""
<div class="emoji">✅</div>
<h1>Password accepted</h1>
<p>Your download should start automatically.</p>
<a class="dl" href="{url}">Download file</a>
<script>window.location.href="{url}";</script>""")


def _error_html(message):
    return _page("Error", f"""
<div class="emoji">⚠️</div>
<h1>Unavailable</h1>
<p>{message}</p>""")
