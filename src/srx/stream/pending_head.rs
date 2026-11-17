//! The SRX header as it is read, before the body begins.

use super::SrxStreamSink;
use crate::{
    Result,
    ResultHead,
};

/// Variables and links read from `<head>`, released to the sink once when
/// the body (`<results>` or `<boolean>`) begins.
#[derive(Default)]
pub(super) struct PendingHead {
    vars: Vec<String>,  // bounded: header width
    links: Vec<String>, // bounded: header links
    sent: bool,
}

impl PendingHead {
    pub(super) fn push_var(&mut self, var: String) {
        self.vars.push(var);
    }

    pub(super) fn push_link(&mut self, href: String) {
        self.links.push(href);
    }

    /// Validate the header and hand it to the sink as a SELECT header,
    /// at most once.
    pub(super) async fn send_select<S>(&mut self, sink: &mut S) -> Result<()>
    where
        S: SrxStreamSink + Send,
    {
        if !self.sent {
            self.sent = true;
            sink.select_header(self.take()?).await?;
        }
        Ok(())
    }

    /// Validate the header as an ASK header.
    pub(super) fn take_ask(&mut self) -> Result<ResultHead> {
        let head = self.take()?;
        head.check_ask()?;
        Ok(head)
    }

    fn take(&mut self) -> Result<ResultHead> {
        Ok(ResultHead::try_new(
            std::mem::take(&mut self.vars),
            std::mem::take(&mut self.links),
        )?)
    }
}
