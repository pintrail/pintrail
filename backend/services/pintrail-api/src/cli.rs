//! Operator commands.
//!
//! Authors are admin-provisioned with no self-service signup (docs/DESIGN.md
//! 1.2), so the very first admin has to come from somewhere outside the HTTP
//! API. This is that somewhere.

use std::io::{IsTerminal, Read};

use sqlx::PgPool;
use uuid::Uuid;

use crate::authors::auth::hash_password;
use crate::authors::model::AuthorRole;

pub const USAGE: &str = "\
usage: pintrail-api [command]

commands:
  (no command)                     run the HTTP server
  healthcheck                      probe the local server's liveness endpoint
                                   and exit 0 or 1; used by the container
  create-author <email> <role>     provision an author; reads the password
                                   from stdin. role: viewer | editor | admin
  reset-password <email>           set a new password, read from stdin

The password is read from stdin rather than taken as an argument because
process arguments are visible to any user on the host via ps.

examples:
  pintrail-api create-author dean@umass.edu admin
  echo \"$PW\" | pintrail-api create-author bot@umass.edu editor
";

/// Probes `/health` over a plain TCP socket and exits accordingly.
///
/// Written by hand rather than pulling in an HTTP client, and used instead of
/// `curl` in the container healthcheck, so the runtime image needs no extra
/// packages — every binary in it is attack surface.
///
/// Deliberately hits `/health` (liveness) rather than `/health/ready`: a
/// database blip should not make an orchestrator kill an otherwise healthy
/// API.
pub fn healthcheck(bind_addr: &str) -> anyhow::Result<()> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    // The configured address may be a wildcard, which is valid to listen on
    // but not to connect to.
    let target = match bind_addr.rsplit_once(':') {
        Some((host, port)) if host.is_empty() || host == "0.0.0.0" || host == "::" || host == "[::]" => {
            format!("127.0.0.1:{port}")
        }
        _ => bind_addr.to_string(),
    };

    let timeout = Duration::from_secs(3);
    let addr: std::net::SocketAddr = target
        .parse()
        .map_err(|e| anyhow::anyhow!("cannot parse {target:?} as an address: {e}"))?;

    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|e| anyhow::anyhow!("connect {target}: {e}"))?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    // HTTP/1.0 so the server closes the connection itself and the read below
    // terminates without needing to parse a content length.
    write!(stream, "GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    stream.flush()?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    if response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200") {
        Ok(())
    } else {
        let status = response.lines().next().unwrap_or("(no response)");
        anyhow::bail!("unhealthy: {status}")
    }
}

pub async fn create_author(db: &PgPool, email: &str, role_str: &str) -> anyhow::Result<()> {
    let role: AuthorRole = role_str
        .parse()
        .map_err(|e: String| anyhow::anyhow!("{e}"))?;

    let email = email.trim();
    if !looks_like_email(email) {
        anyhow::bail!("{email:?} does not look like an email address");
    }

    let password = read_password("Password for new author: ")?;
    validate_password(&password)?;

    let hash = hash_password(&password).map_err(|e| anyhow::anyhow!("{e}"))?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO authors (email, password_hash, role)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
    )
    .bind(email)
    .bind(&hash)
    .bind(role)
    .fetch_one(db)
    .await
    .map_err(|e| match &e {
        // The unique index is on lower(email), so this catches case variants
        // of an existing address too.
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            anyhow::anyhow!("an author with email {email:?} already exists")
        }
        _ => e.into(),
    })?;

    println!("created author {id} <{email}> with role {}", role.as_str());
    Ok(())
}

pub async fn reset_password(db: &PgPool, email: &str) -> anyhow::Result<()> {
    let email = email.trim();

    let password = read_password("New password: ")?;
    validate_password(&password)?;

    let hash = hash_password(&password).map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut tx = db.begin().await?;

    let id: Option<Uuid> = sqlx::query_scalar(
        "UPDATE authors SET password_hash = $1 WHERE lower(email) = lower($2) RETURNING id",
    )
    .bind(&hash)
    .bind(email)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(id) = id else {
        anyhow::bail!("no author with email {email:?}");
    };

    // A password reset must not leave old sessions alive -- the usual reason
    // to reset is that the credential may be compromised.
    let revoked = sqlx::query("DELETE FROM author_sessions WHERE author_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    tx.commit().await?;

    println!("password updated for {email}; revoked {revoked} active session(s)");
    Ok(())
}

/// Reads a password from stdin, without echo when attached to a terminal.
/// Piped input works too, which is what makes this scriptable.
fn read_password(prompt: &str) -> anyhow::Result<String> {
    let stdin = std::io::stdin();

    if stdin.is_terminal() {
        eprint!("{prompt}");
        let pw = rpassword::read_password()?;
        Ok(pw)
    } else {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf.trim_end_matches(['\n', '\r']).to_string())
    }
}

/// Minimum bar only. Authors are a small trusted group, so this guards against
/// fat-fingering an empty password rather than trying to enforce a policy.
fn validate_password(password: &str) -> anyhow::Result<()> {
    if password.chars().count() < 12 {
        anyhow::bail!("password must be at least 12 characters");
    }
    Ok(())
}

fn looks_like_email(candidate: &str) -> bool {
    // Deliberately loose: full RFC 5322 validation rejects addresses that
    // work in practice, and the real check is whether mail arrives.
    match candidate.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !candidate.contains(char::is_whitespace)
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plausible_addresses() {
        assert!(looks_like_email("dean@umass.edu"));
        assert!(looks_like_email("a.b+tag@mail.example.co.uk"));
    }

    #[test]
    fn rejects_implausible_addresses() {
        assert!(!looks_like_email("no-at-sign"));
        assert!(!looks_like_email("@umass.edu"));
        assert!(!looks_like_email("dean@umass"));
        assert!(!looks_like_email("dean@.edu"));
        assert!(!looks_like_email("dean @umass.edu"));
        assert!(!looks_like_email(""));
    }

    #[test]
    fn password_length_is_enforced() {
        assert!(validate_password("short").is_err());
        assert!(validate_password("exactly12chr").is_ok());
    }
}
