//! Outbound transactional email.
//!
//! [`Mailer`] is the interface the reader flows depend on; two implementations
//! back it. [`LogMailer`] (`MAILER=log`) writes messages to the log for
//! development. [`SmtpMailer`] (`MAILER=smtp`) delivers over SMTP, and so works
//! with Resend, Brevo, Amazon SES, a campus relay -- anything speaking SMTP --
//! by changing only environment variables.
//!
//! `send` is synchronous and called from async handlers, so it must not block
//! on the network. `SmtpMailer::send` therefore only enqueues onto a channel
//! and returns; a background task owns the SMTP connection and does the actual
//! delivery. A registration returns immediately regardless of mail latency,
//! and a transient relay failure never surfaces to the user (which also keeps
//! the response identical whether or not the address exists -- see the reader
//! registration flow).

use std::sync::Arc;

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::{AsyncTransport, Message, Tokio1Executor};

use crate::config::Settings;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body: String,
}

pub trait Mailer: Send + Sync + std::fmt::Debug {
    fn send(&self, email: Email) -> AppResult<()>;
}

pub type SharedMailer = Arc<dyn Mailer>;

/// Development mailer: logs the message, including the verification link, so
/// the flow is exercisable end to end without a provider.
///
/// Logging a live credential is acceptable only because this is explicitly
/// the non-production implementation; it warns loudly at startup so it cannot
/// be deployed by accident.
#[derive(Debug)]
pub struct LogMailer;

impl Mailer for LogMailer {
    fn send(&self, email: Email) -> AppResult<()> {
        tracing::warn!(
            to = %email.to,
            subject = %email.subject,
            "MAIL NOT SENT (LogMailer). Message body follows:\n{}",
            email.body
        );
        Ok(())
    }
}

/// TLS mode for the SMTP connection.
#[derive(Debug, Clone, Copy)]
enum TlsMode {
    /// STARTTLS on the submission port (587). The usual choice.
    StartTls,
    /// Implicit TLS from the first byte (465, SMTPS).
    Implicit,
    /// Plaintext. Only for a local mail catcher; rejected in production by any
    /// real relay.
    None,
}

impl TlsMode {
    fn parse(s: &str) -> AppResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "starttls" => Ok(TlsMode::StartTls),
            "implicit" | "smtps" => Ok(TlsMode::Implicit),
            "none" | "plain" => Ok(TlsMode::None),
            other => Err(AppError::Internal(anyhow::anyhow!(
                "unknown SMTP_TLS {other:?} (expected starttls, implicit, or none)"
            ))),
        }
    }
}

/// SMTP mailer. Holds only the channel sender; the transport lives in the
/// background task started by [`SmtpMailer::start`].
#[derive(Debug)]
pub struct SmtpMailer {
    tx: tokio::sync::mpsc::UnboundedSender<Email>,
}

impl SmtpMailer {
    /// Builds the transport, spawns the delivery task, and returns a handle.
    ///
    /// Must be called from within the tokio runtime (it is -- `AppState::connect`
    /// is async). The transport is validated here so a misconfiguration fails
    /// at startup rather than on the first registration.
    fn start(
        host: &str,
        port: u16,
        tls: TlsMode,
        credentials: Option<Credentials>,
        from: Mailbox,
    ) -> AppResult<Self> {
        let mut builder = match tls {
            TlsMode::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
                .map_err(|e| smtp_err("configure STARTTLS relay", e))?,
            TlsMode::Implicit => AsyncSmtpTransport::<Tokio1Executor>::relay(host)
                .map_err(|e| smtp_err("configure implicit-TLS relay", e))?,
            // builder_dangerous is lettre's own name for the no-TLS path; the
            // danger is real, which is why this mode exists only for a catcher.
            TlsMode::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
        }
        .port(port);

        if let Some(creds) = credentials {
            builder = builder.credentials(creds);
        }

        let transport = builder.build();

        // Unbounded so `send` never blocks or fails on backpressure. At this
        // volume (verification + reset only) the queue stays short; if it ever
        // needs bounding, that is where a real send failure should surface.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Email>();

        tokio::spawn(async move {
            while let Some(email) = rx.recv().await {
                deliver(&transport, &from, email).await;
            }
            tracing::info!("SMTP delivery task stopped (channel closed)");
        });

        Ok(Self { tx })
    }
}

impl Mailer for SmtpMailer {
    fn send(&self, email: Email) -> AppResult<()> {
        // Only fails if the delivery task has died, which is a genuine internal
        // fault -- transient relay errors are handled in the background.
        self.tx.send(email).map_err(|_| {
            AppError::Internal(anyhow::anyhow!("SMTP delivery task is not running"))
        })
    }
}

/// Sends one message, with a couple of retries for transient faults. Logs and
/// drops on final failure -- there is no user request to return an error to,
/// and a durable retry queue is out of scope for this volume.
async fn deliver(transport: &AsyncSmtpTransport<Tokio1Executor>, from: &Mailbox, email: Email) {
    let to: Mailbox = match email.to.parse() {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(to = %email.to, error = %e, "dropping email: unparseable recipient");
            return;
        }
    };

    let message = match Message::builder()
        .from(from.clone())
        .to(to)
        .subject(&email.subject)
        .body(email.body.clone())
    {
        Ok(m) => m,
        Err(e) => {
            tracing::error!(error = %e, "dropping email: could not build message");
            return;
        }
    };

    for attempt in 1..=3 {
        match transport.send(message.clone()).await {
            Ok(_) => {
                tracing::info!(to = %email.to, subject = %email.subject, "email sent");
                return;
            }
            Err(e) if attempt < 3 => {
                tracing::warn!(to = %email.to, attempt, error = %e, "email send failed; retrying");
                tokio::time::sleep(std::time::Duration::from_secs(2 * attempt)).await;
            }
            Err(e) => {
                tracing::error!(to = %email.to, error = %e, "email send failed after retries; dropped");
            }
        }
    }
}

fn smtp_err(what: &str, e: impl std::fmt::Display) -> AppError {
    AppError::Internal(anyhow::anyhow!("SMTP: could not {what}: {e}"))
}

/// Chooses a mailer from configuration.
pub fn build_mailer(settings: &Settings) -> AppResult<SharedMailer> {
    match settings.mailer.as_str() {
        "log" => {
            tracing::warn!(
                "MAILER=log: verification and password-reset emails will be written to \
                 the log instead of sent. Do not run this configuration in production."
            );
            Ok(Arc::new(LogMailer))
        }
        "smtp" => {
            let host = settings.smtp_host.as_deref().ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("MAILER=smtp requires SMTP_HOST"))
            })?;
            let tls = TlsMode::parse(&settings.smtp_tls)?;
            let from: Mailbox = settings.mail_from.parse().map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "MAIL_FROM {:?} is not a valid address: {e}",
                    settings.mail_from
                ))
            })?;

            // Credentials are optional: a local catcher and some relays accept
            // unauthenticated submission.
            let credentials = match (&settings.smtp_username, &settings.smtp_password) {
                (Some(u), Some(p)) => Some(Credentials::new(u.clone(), p.clone())),
                (None, None) => None,
                _ => {
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "set both SMTP_USERNAME and SMTP_PASSWORD, or neither"
                    )))
                }
            };

            tracing::info!(
                host, port = settings.smtp_port, tls = settings.smtp_tls,
                authenticated = credentials.is_some(), from = %settings.mail_from,
                "MAILER=smtp: sending transactional email over SMTP"
            );

            Ok(Arc::new(SmtpMailer::start(
                host,
                settings.smtp_port,
                tls,
                credentials,
                from,
            )?))
        }
        other => Err(AppError::Internal(anyhow::anyhow!(
            "unknown MAILER {other:?} (supported: log, smtp)"
        ))),
    }
}

/// Builds the verification message. Kept here so wording and link format live
/// in one place rather than being inlined at each call site.
pub fn verification_email(to: &str, link: &str) -> Email {
    Email {
        to: to.to_string(),
        subject: "Verify your Pintrail account".to_string(),
        body: format!(
            "Welcome to Pintrail.\n\n\
             Confirm this address to start commenting and building trails:\n\n\
             {link}\n\n\
             This link expires in 24 hours. If you did not create an account, \
             you can ignore this message."
        ),
    }
}

pub fn password_reset_email(to: &str, link: &str) -> Email {
    Email {
        to: to.to_string(),
        subject: "Reset your Pintrail password".to_string(),
        body: format!(
            "Someone asked to reset the password for this Pintrail account.\n\n\
             {link}\n\n\
             This link expires in 1 hour and can be used once. If this was not \
             you, no action is needed -- your password has not changed."
        ),
    }
}

/// Sent when someone tries to register an address that already has an
/// account. The registration endpoint returns the same response either way to
/// avoid disclosing who is registered, so this is what tells the actual owner
/// that something happened.
pub fn duplicate_registration_email(to: &str) -> Email {
    Email {
        to: to.to_string(),
        subject: "Someone tried to register your Pintrail account".to_string(),
        body:
            "Someone attempted to create a Pintrail account with this email address, \
             but one already exists.\n\n\
             If that was you, try signing in instead, or use the password reset flow \
             if you have forgotten your password.\n\n\
             If it was not you, no action is needed."
                .to_string(),
    }
}
