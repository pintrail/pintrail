//! Outbound transactional email.
//!
//! DESIGN.md requires email verification but never specifies a delivery
//! mechanism, and the repository has no SMTP configuration. Rather than pick
//! a provider by default, this defines the seam: [`Mailer`] is the interface
//! the reader flows depend on, and [`LogMailer`] is the development
//! implementation that writes the message to the log instead of sending it.
//!
//! Adding a real provider means one more `impl Mailer` and a config branch in
//! [`build_mailer`] -- no changes to the routes.

use std::sync::Arc;

use crate::error::AppResult;

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

/// Chooses a mailer from configuration. Only the development mailer exists
/// today; a real provider slots in here.
pub fn build_mailer(kind: &str) -> AppResult<SharedMailer> {
    match kind {
        "log" => {
            tracing::warn!(
                "MAILER=log: verification and password-reset emails will be written to \
                 the log instead of sent. Do not run this configuration in production."
            );
            Ok(Arc::new(LogMailer))
        }
        other => Err(crate::error::AppError::Internal(anyhow::anyhow!(
            "unknown MAILER {other:?} (supported: log)"
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
