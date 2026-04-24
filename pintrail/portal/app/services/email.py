import logging
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText

import aiosmtplib

from app.config import settings

logger = logging.getLogger(__name__)

_CONFIRM_TEXT = """\
Hi there,

You're on the list! We'll send you one email when PinTrail launches on iOS and Android.

See you on the trail,
The PinTrail Team · UMass Amherst
"""

_CONFIRM_HTML = """\
<!DOCTYPE html>
<html>
<body style="font-family:'DM Sans',sans-serif;background:#f5f0e8;padding:40px 0;margin:0">
  <div style="max-width:480px;margin:0 auto;background:#faf8f3;padding:40px;border:1px solid rgba(61,90,46,0.12)">
    <p style="font-family:'Space Mono',monospace;font-size:11px;letter-spacing:0.2em;text-transform:uppercase;color:#7a9b6a;margin:0 0 20px">PinTrail · UMass Amherst</p>
    <h1 style="font-size:22px;font-weight:900;color:#2a3d1f;margin:0 0 16px">You're on the list!</h1>
    <p style="color:#2a3d1f;opacity:0.7;line-height:1.7;margin:0 0 24px">
      We'll send you one email when PinTrail launches on iOS and Android. No spam — just the launch announcement.
    </p>
    <p style="color:#2a3d1f;opacity:0.7;line-height:1.7;margin:0">
      See you on the trail,<br>
      <strong>The PinTrail Team</strong>
    </p>
    <div style="margin-top:32px;padding-top:24px;border-top:1px solid rgba(61,90,46,0.12)">
      <p style="font-family:'Space Mono',monospace;font-size:10px;color:#7a9b6a;margin:0">
        © 2025 PinTrail · UMass Amherst
      </p>
    </div>
  </div>
</body>
</html>
"""


async def send_notify_confirmation(to_email: str) -> None:
    if not settings.notify_email_enabled:
        logger.info("Email disabled — skipping confirmation to %s", to_email)
        return

    msg = MIMEMultipart("alternative")
    msg["Subject"] = "You're on the PinTrail list!"
    msg["From"] = settings.smtp_from
    msg["To"] = to_email
    msg.attach(MIMEText(_CONFIRM_TEXT, "plain"))
    msg.attach(MIMEText(_CONFIRM_HTML, "html"))

    try:
        await aiosmtplib.send(
            msg,
            hostname=settings.smtp_host,
            port=settings.smtp_port,
            username=settings.smtp_user,
            password=settings.smtp_password,
            start_tls=True,
        )
        logger.info("Confirmation email sent to %s", to_email)
    except Exception:
        logger.exception("Failed to send confirmation email to %s", to_email)
