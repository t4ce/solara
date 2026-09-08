//! Browser-owned script/fetch scheduling. Each navigation drops all old work.
use rust_qjs_dom::DomArtifact;
use solara::{
    page_runtime::{PageRuntime, Script},
    spec_layout::{NodeId, SpecLayout},
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
type Fetch = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>>>>;
pub(crate) struct Scripts {
    realm: PageRuntime,
    script: Option<(String, Fetch)>,
    fetching: Vec<(u64, Fetch)>,
    started: trueos::clock::Instant,
}
impl Scripts {
    pub fn new(artifact: &DomArtifact, layout: &SpecLayout) -> Result<Self, String> {
        Ok(Self {
            realm: PageRuntime::new(artifact, layout)?,
            script: None,
            fetching: Vec::new(),
            started: trueos::clock::Instant::now(),
        })
    }
    pub fn click(&mut self, node: NodeId) -> Result<(), String> {
        self.realm.click(node)
    }
    pub fn tick(&mut self, layout: &mut SpecLayout) -> Result<bool, String> {
        if let Some((url, future)) = &mut self.script {
            if let Poll::Ready(result) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                let source = String::from_utf8(result?).map_err(|e| e.to_string())?;
                self.realm.execute(&source, url)?;
                self.realm.scripts.pop_front();
                self.script = None;
            }
        }
        // Classic/deferred tags execute in document order after tree construction.
        for _ in 0..4 {
            if self.script.is_some() {
                break;
            }
            match self.realm.scripts.front().cloned() {
                Some(Script::Inline(source)) => {
                    self.realm.execute(&source, "<inline>")?;
                    self.realm.scripts.pop_front();
                }
                Some(Script::External(url)) => {
                    self.script = Some((
                        url.clone(),
                        Box::pin(crate::native_images::fetch_bytes_limited(
                            url,
                            4 * 1024 * 1024,
                        )),
                    ));
                }
                None => break,
            }
        }
        while self.fetching.len() < 4 {
            let Some(request) = self.realm.take_request() else {
                break;
            };
            self.fetching.push((
                request.id,
                Box::pin(crate::native_images::fetch_bytes_limited(
                    request.url,
                    4 * 1024 * 1024,
                )),
            ));
        }
        let mut i = 0;
        while i < self.fetching.len() {
            if let Poll::Ready(result) = self.fetching[i]
                .1
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                let (id, _) = self.fetching.remove(i);
                crate::parser_probe::report_info(format_args!(
                    "solara: page-fetch-complete id={id}"
                ));
                self.realm.complete(
                    id,
                    result.and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string())),
                )?;
                crate::parser_probe::report_info(format_args!(
                    "solara: page-fetch-delivered id={id}"
                ));
            } else {
                i += 1;
            }
        }
        self.realm.tick(
            trueos::clock::Instant::now()
                .saturating_duration_since(self.started)
                .as_millis() as u64,
            layout,
        )
    }
}
