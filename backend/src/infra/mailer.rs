use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};

use crate::{config::SmtpConfig, error::AppResult};

#[derive(Clone, Debug)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, email: Email) -> AppResult<()>;
}

pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpMailer {
    pub fn new(cfg: &SmtpConfig) -> anyhow::Result<Self> {
        let transport = match (&cfg.username, &cfg.password) {
            (Some(user), Some(pass)) => AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)?
                .port(cfg.port)
                .credentials(Credentials::new(user.clone(), pass.clone()))
                .build(),
            // Local dev (Mailpit): plain SMTP, no auth.
            _ => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host)
                .port(cfg.port)
                .build(),
        };
        Ok(Self {
            transport,
            from: cfg.from.parse()?,
        })
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, email: Email) -> AppResult<()> {
        let message = Message::builder()
            .from(self.from.clone())
            .to(email.to.parse().map_err(anyhow::Error::from)?)
            .subject(email.subject)
            .body(email.body)
            .map_err(anyhow::Error::from)?;
        self.transport
            .send(message)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(())
    }
}
