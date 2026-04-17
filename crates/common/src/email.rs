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
    Noop,
}

#[derive(Clone)]
pub struct ConsoleConfig {
    /// Rendered as the `From:` line in the pretty-printed summary.
    pub from: String,
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

    /// Short name for logs and health-check responses.
    pub fn transport_name(&self) -> &'static str {
        match self {
            EmailSender::Console(_) => "console",
            EmailSender::Noop => "noop",
        }
    }

    pub async fn send(&self, msg: EmailMessage) -> Result<(), EmailError> {
        match self {
            EmailSender::Console(cfg) => send_console(cfg, msg),
            EmailSender::Noop => Ok(()),
        }
    }
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

/// Email-verification link body. `url` is the fully-qualified confirm URL.
pub fn render_email_verification(url: &str) -> (String, String) {
    let subject = "Confirm your MG Custody email".to_string();
    let body = format!(
        "Thanks for creating an MG Custody account.\n\
         \n\
         Click this link to confirm your email address:\n\
         {url}\n\
         \n\
         The link expires in 24 hours. If you didn't register, ignore\n\
         this message.\n\
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
    fn verification_template_contains_url() {
        let (subject, body) = render_email_verification("https://custody.local/verify?t=xyz");
        assert!(subject.contains("Confirm"));
        assert!(body.contains("https://custody.local/verify?t=xyz"));
    }
}
