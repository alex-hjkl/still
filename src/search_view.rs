use super::*;

impl PlayerView {
    pub(super) fn search_changed(
        &mut self,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let searching = matches!(self.library.browser().state().page, Page::Search(_));
        if text.trim().is_empty() {
            if searching {
                self.back(cx);
                self.focus_library = false;
                window.focus(&self.search.focus_handle(cx));
            }
            return;
        }
        if searching {
            if !self.library.set_search(text, cx) {
                return;
            }
            self.search_delay = None;
            self.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
        } else {
            self.remember_folder(cx);
            self.library.navigate(Page::Search(text), true, cx);
            self.reset_navigation_ui(cx);
            self.focus_library = false;
            window.focus(&self.search.focus_handle(cx));
        }
        self.update_columns(cx);
        if self.connection.read(cx).session().is_demo() {
            self.submit_search(cx);
            return;
        }
        let request_id = self.library.request_id(cx);
        let timer = cx.background_executor().timer(Duration::from_millis(250));
        self.search_delay = Some(cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                if this.library.request_id(cx) == request_id {
                    this.submit_search(cx);
                }
            });
        }));
    }

    pub(super) fn submit_search(&mut self, cx: &mut Context<Self>) {
        if matches!(self.library.browser().state().page, Page::Search(_))
            && !self.library.browser().state().loaded()
            && !self.library.is_loading(cx)
        {
            self.search_delay = None;
            let context = self.library_context(cx);
            if let Some(outcome) = self.library.load_current(context, cx) {
                self.accept_library(outcome, cx);
            }
            cx.notify();
        }
    }
}
