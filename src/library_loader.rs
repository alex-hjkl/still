//! Owns pending library work and delivers completions without borrowing a view.
use crate::{
    api::Navidrome,
    browser::{LibraryData, LibraryRequest},
    requests::Cancellation,
};
use gpui::{Context, EventEmitter, Task};

pub struct LibraryReady;
pub struct LoadContext {
    pub request_id: u64,
    pub request: LibraryRequest,
    pub like_revision: u64,
    pub playback_generation: u64,
}
pub struct LoadOutcome {
    pub context: LoadContext,
    pub result: anyhow::Result<LibraryData>,
}
#[derive(Default)]
pub struct LibraryLoader {
    task: Option<Task<()>>,
    cancellation: Option<Cancellation>,
    outcome: Option<LoadOutcome>,
}
impl EventEmitter<LibraryReady> for LibraryLoader {}
impl LibraryLoader {
    pub fn cancel(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
        self.task = None;
        self.outcome = None;
    }
    pub fn take_outcome(&mut self) -> Option<LoadOutcome> {
        self.outcome.take()
    }
    pub fn start(
        &mut self,
        client: Navidrome,
        context: LoadContext,
        cancellation: Cancellation,
        cx: &mut Context<Self>,
    ) {
        self.cancel();
        self.cancellation = Some(cancellation.clone());
        let operation = context.request.clone();
        let task = cx
            .background_executor()
            .spawn(async move { operation.run(client, &cancellation) });
        self.task = Some(cx.spawn(async move |loader, cx| {
            let result = task.await;
            let _ = loader.update(cx, |loader, cx| {
                loader.outcome = Some(LoadOutcome { context, result });
                cx.emit(LibraryReady);
            });
        }));
    }
}
impl Drop for LibraryLoader {
    fn drop(&mut self) {
        self.cancel();
    }
}
