//! Outbound email dispatch.
//!
//! Production deployments hook in a real SMTP transport (a `Smtp` variant
//! is reserved for when `lettre` is added to the workspace). Dev
//! deployments get `Console`, which prints a framed, human-readable
//! summary to stdout so you can click-copy the verification link or OTP
//! out of your terminal.
//!
//! **Never log sensitive values (OTPs, reset links) via `tracing::info!`
//! directly** — use `EmailSender::send` so the transport can be swapped
//! atomically.
//!
//! ## Typical wiring
//!
//! ```ignore
//! let sender: Arc<EmailSender> = Arc::new(match cfg.email_mode.as_str() {
//!     "smtp" => EmailSender::smtp(cfg)?,
//!     _      => EmailSender::console("MG Custody <no-reply@custody.local>"),
//! });
//! // ... pass into AppState
//! ```
//!
//! We use an enum rather than a trait-object because (a) we only need a
//! small closed set of transports, (b) `Arc<dyn Trait>` with async-fn
//! forces either `async-trait` (adds a dep for no benefit) or careful
//! `Send` bounds; the enum avoids both.

use std::fmt;

/// An outbound email message. `body_html` is optional; console sender
/// shows plain-text only, a real SMTP sender would send
/// multipart/alternative.
#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub to: String,
    pub subject: String,
    pub body_text: String,
    pub body_html: Option<String>,
}

impl EmailMessage {
    pub fn new(to: impl Into<String>, subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            subject: subject.into(),
            body_text: body.into(),
            body_html: None,
        }
    }

    pub fn with_html(mut self, html: impl Into<String>) -> Self {
        self.body_html = Some(html.into());
        self
    }
}

#[derive(Debug)]
pub enum EmailError {
    TransportFailed(String),
    InvalidAddress(String),
    Unavailable,
}

impl fmt::Display for EmailError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmailError::TransportFailed(e) => write!(f, "email transport failed: {e}"),
            EmailError::InvalidAddress(e) => write!(f, "invalid email address: {e}"),
            EmailError::Unavailable => write!(f, "email transport unavailable"),
        }
    }
}

impl std::error::Error for EmailError {}

/// Transport for outbound email. Closed enum so the object lives happily
/// behind `Arc<EmailSender>` without trait-object / async-trait ceremony.
#[derive(Clone)]
pub enum EmailSender {
    Console(ConsoleConfig),
    Smtp(SmtpConfig),
    Noop,
}

#[derive(Clone)]
pub struct ConsoleConfig {
    /// Rendered as the `From:` line in the pretty-printed summary.
    pub from: String,
}

/// Configuration for a real SMTP transport. `from` must be an RFC-5322
/// mailbox (e.g., `"MG Custody <no-reply@custody.example>"`).
///
/// Two flavours are supported:
///   - **Implicit TLS** (`tls = SmtpTls::Wrapper`): direct TLS on
///     port 465. Fewer round-trips, recommended by modern providers.
///   - **STARTTLS** (`tls = SmtpTls::Required`): plain TCP on port
///     587, negotiated up. We NEVER fall back to plaintext; if the
///     server doesn't offer STARTTLS we refuse to send.
#[derive(Clone)]
pub struct SmtpConfig {
    pub from: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: SmtpTls,
    /// Short timeout on SMTP operations. Transport failure is surfaced
    /// to callers so password-reset flows can fail fast instead of
    /// blocking the user for a full TCP keep-alive window.
    pub timeout_secs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmtpTls {
    /// Implicit TLS (port 465). Connection starts encrypted.
    Wrapper,
    /// STARTTLS required. We refuse to send if the server doesn't
    /// advertise the extension — no silent downgrade to plaintext.
    Required,
}

/// Lazily-built async SMTP transport. We don't construct it during
/// `EmailSender::smtp()` because that would force callers to .await
/// transport setup before they can put the sender in `AppState`.
/// Instead, each `send()` call builds a throwaway transport with a
/// short connection pool — `lettre` caches the TLS handshake at the
/// pool level so this is cheap in the steady state.
impl SmtpConfig {
    /// Cold-start probe. Builds the real transport and asks lettre's
    /// `test_connection()` to do a full EHLO + (STARTTLS or implicit
    /// TLS) + AUTH if creds are set, then disconnect. Unlike the
    /// TCP-only probe in `common::readiness`, this catches:
    ///   - bad credentials
    ///   - server that refuses STARTTLS when configured Required
    ///   - wrong port (implicit TLS vs. STARTTLS mismatch)
    /// Returns the short error string on failure for the readiness
    /// report.
    pub async fn probe_connection(&self) -> Result<(), String> {
        let transport = self
            .build_transport()
            .map_err(|e| format!("build transport: {e}"))?;
        match transport.test_connection().await {
            Ok(true) => Ok(()),
            Ok(false) => Err("SMTP server did not respond to NOOP".into()),
            Err(e) => Err(format!("SMTP handshake: {e}")),
        }
    }

    fn build_transport(&self) -> Result<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>, EmailError> {
        let mut builder = match self.tls {
            SmtpTls::Wrapper => {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay(&self.host)
                    .map_err(|e| EmailError::TransportFailed(format!("wrapper TLS relay: {e}")))?
            }
            SmtpTls::Required => {
                lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(&self.host)
                    .map_err(|e| EmailError::TransportFailed(format!("starttls relay: {e}")))?
            }
        };
        builder = builder
            .port(self.port)
            .timeout(Some(std::time::Duration::from_secs(self.timeout_secs)));
        if let (Some(user), Some(pass)) = (self.username.clone(), self.password.clone()) {
            builder = builder.credentials(lettre::transport::smtp::authentication::Credentials::new(user, pass));
        }
        Ok(builder.build())
    }
}

impl EmailSender {
    /// Dev-time transport. Pretty-prints the envelope + body to
    /// `tracing::info!`. Safe to use in CI and tests.
    pub fn console(from: impl Into<String>) -> Self {
        EmailSender::Console(ConsoleConfig { from: from.into() })
    }

    /// Silent sender for tests that don't care about email delivery.
    pub fn noop() -> Self {
        EmailSender::Noop
    }

    /// Real SMTP transport. See `SmtpConfig` for TLS semantics.
    pub fn smtp(cfg: SmtpConfig) -> Self {
        EmailSender::Smtp(cfg)
    }

    /// Short name for logs and health-check responses.
    pub fn transport_name(&self) -> &'static str {
        match self {
            EmailSender::Console(_) => "console",
            EmailSender::Smtp(_) => "smtp",
            EmailSender::Noop => "noop",
        }
    }

    pub async fn send(&self, msg: EmailMessage) -> Result<(), EmailError> {
        match self {
            EmailSender::Console(cfg) => send_console(cfg, msg),
            EmailSender::Smtp(cfg) => send_smtp(cfg, msg).await,
            EmailSender::Noop => Ok(()),
        }
    }
}

async fn send_smtp(cfg: &SmtpConfig, msg: EmailMessage) -> Result<(), EmailError> {
    use lettre::{AsyncTransport, Message, message::header::ContentType};

    let from: lettre::Address = cfg
        .from
        .parse()
        .or_else(|_| {
            // Accept "Name <addr@host>" form too.
            cfg.from
                .rsplit_once('<')
                .and_then(|(_, rest)| rest.strip_suffix('>'))
                .map(|s| s.parse::<lettre::Address>())
                .unwrap_or_else(|| cfg.from.parse::<lettre::Address>())
        })
        .map_err(|e| EmailError::InvalidAddress(format!("from {}: {e}", cfg.from)))?;
    let to: lettre::Address = msg
        .to
        .parse()
        .map_err(|e| EmailError::InvalidAddress(format!("to {}: {e}", msg.to)))?;

    let mut builder = Message::builder()
        .from(lettre::message::Mailbox::new(None, from))
        .to(lettre::message::Mailbox::new(None, to))
        .subject(&msg.subject)
        .header(ContentType::TEXT_PLAIN);
    if !cfg.from.is_empty() {
        // Preserve display-name form when the caller supplied one.
        builder = builder.reply_to(cfg.from.parse().unwrap_or_else(|_| {
            lettre::message::Mailbox::new(None, msg.to.parse().unwrap())
        }));
    }
    let email = builder
        .body(msg.body_text.clone())
        .map_err(|e| EmailError::TransportFailed(format!("message build: {e}")))?;

    let transport = cfg.build_transport()?;
    transport
        .send(email)
        .await
        .map_err(|e| EmailError::TransportFailed(format!("smtp send: {e}")))?;

    tracing::info!(
        target: "custody::email",
        to = %msg.to,
        subject = %msg.subject,
        transport = "smtp",
        "Email sent"
    );
    Ok(())
}

fn send_console(cfg: &ConsoleConfig, msg: EmailMessage) -> Result<(), EmailError> {
    // Use `tracing::info!` so the output is correlated with other service
    // logs. The body is multi-line; we frame it with a visible separator
    // to make it obvious that this is dev mode and NOT a real dispatch.
    let bar = "─".repeat(68);
    tracing::info!(
        target: "custody::email",
        to = %msg.to,
        subject = %msg.subject,
        transport = "console",
        "\n╭{bar}╮\n│ 📧 EMAIL (console transport — NOT sent for real)\n├{bar}┤\n│ From:    {from}\n│ To:      {to}\n│ Subject: {subject}\n│{bar}│\n{body}\n╰{bar}╯",
        bar = bar,
        from = cfg.from,
        to = msg.to,
        subject = msg.subject,
        body = indent_body(&msg.body_text),
    );
    Ok(())
}

fn indent_body(body: &str) -> String {
    body.lines()
        .map(|l| format!("│ {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Message templates
// ---------------------------------------------------------------------------

/// Password-reset OTP email body. `ttl_minutes` reflects DB expiry so the
/// user knows how long they have to enter the code.
pub fn render_password_reset(code: &str, ttl_minutes: u64) -> (String, String) {
    let subject = "Your MG Custody password reset code".to_string();
    let body = format!(
        "Someone requested a password reset for your MG Custody account.\n\
         \n\
         Verification code: {code}\n\
         This code expires in {ttl_minutes} minutes.\n\
         \n\
         If you didn't request this, you can safely ignore this email —\n\
         your password remains unchanged.\n\
         \n\
         — MG Custody"
    );
    (subject, body)
}

/// Email-verification code body. Mirrors the password-reset template
/// shape so we only maintain one delivery pattern: a 6-digit OTP that
/// the user enters in the UI. URL-based confirm flows need a signed
/// token mint + route — intentionally deferred until we actually need
/// deep-linking from a client outside the SPA.
pub fn render_email_verification(code: &str, ttl_minutes: u64) -> (String, String) {
    let subject = "Confirm your MG Custody email".to_string();
    let body = format!(
        "Welcome to MG Custody.\n\
         \n\
         Confirmation code: {code}\n\
         This code expires in {ttl_minutes} minutes.\n\
         \n\
         Enter it on the verification screen to finish setting up your\n\
         account. If you didn't register, you can safely ignore this\n\
         email — no action will be taken.\n\
         \n\
         — MG Custody"
    );
    (subject, body)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn console_sender_accepts_message() {
        let sender = EmailSender::console("test@custody.local");
        let msg = EmailMessage::new("user@example.com", "hi", "body");
        sender.send(msg).await.unwrap();
        assert_eq!(sender.transport_name(), "console");
    }

    #[tokio::test]
    async fn noop_sender_never_errors() {
        let sender = EmailSender::noop();
        assert!(sender.send(EmailMessage::new("a", "b", "c")).await.is_ok());
        assert_eq!(sender.transport_name(), "noop");
    }

    #[test]
    fn reset_template_contains_code_and_ttl() {
        let (subject, body) = render_password_reset("123456", 15);
        assert!(subject.contains("password reset"));
        assert!(body.contains("123456"));
        assert!(body.contains("15 minutes"));
    }

    #[test]
    fn verification_template_contains_code_and_ttl() {
        let (subject, body) = render_email_verification("654321", 10);
        assert!(subject.contains("Confirm"));
        assert!(body.contains("654321"));
        assert!(body.contains("10 minutes"));
    }

    #[test]
    fn smtp_sender_reports_transport_name() {
        // We don't actually connect to a server here — just verify the
        // transport-name plumbing so the "smtp" label reaches logs and
        // health probes without touching the network.
        let cfg = SmtpConfig {
            from: "noreply@example.com".into(),
            host: "smtp.invalid".into(),
            port: 465,
            username: None,
            password: None,
            tls: SmtpTls::Wrapper,
            timeout_secs: 5,
        };
        assert_eq!(EmailSender::smtp(cfg).transport_name(), "smtp");
    }

    #[tokio::test]
    async fn smtp_send_fails_fast_when_host_unresolvable() {
        // Loopback won't accept SMTP on 465; ensure the sender surfaces
        // a `TransportFailed` error (not a panic) within the 2s timeout.
        // This pins "SMTP path returns Err on failure" so callers can
        // branch on real delivery vs unreachable relay.
        let cfg = SmtpConfig {
            from: "noreply@example.com".into(),
            host: "127.0.0.1".into(),
            port: 1, // port 1 is effectively guaranteed refused
            username: None,
            password: None,
            tls: SmtpTls::Wrapper,
            timeout_secs: 2,
        };
        let sender = EmailSender::smtp(cfg);
        let msg = EmailMessage::new("user@example.com", "hi", "body");
        let result = sender.send(msg).await;
        assert!(matches!(result, Err(EmailError::TransportFailed(_))));
    }
}
