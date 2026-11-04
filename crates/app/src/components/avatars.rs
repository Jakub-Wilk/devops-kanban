//! Serves user avatars to the webview at `/avatars/?d=<descriptor>`. ADO's Graph API needs the
//! PAT, so the page can't load them itself: this handler answers from memory, else the disk
//! cache ([`kanban_core::avatars`]), else ADO. Concurrent requests for one person share a single
//! fetch, and a failed person isn't retried this session. Failures answer 404, and the `Avatar`
//! widget keeps showing initials.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use dioxus::desktop::wry::RequestAsyncResponder;
use dioxus::desktop::wry::http::{Response, StatusCode, header};
use dioxus::desktop::{AssetRequest, use_asset_handler};
use dioxus::prelude::*;
use kanban_core::avatars;

use crate::state::{AppState, use_app};

/// The `src` of an avatar image.
pub fn avatar_src(descriptor: &str) -> String {
    let d: String = url::form_urlencoded::byte_serialize(descriptor.as_bytes()).collect();
    format!("/avatars/?d={d}")
}

#[derive(Default)]
struct Store {
    /// `None`: fetching failed.
    images: HashMap<String, Option<Rc<Vec<u8>>>>,
    /// Requests waiting on a fetch in flight.
    waiting: HashMap<String, Vec<RequestAsyncResponder>>,
}

#[component]
pub fn AvatarServer() -> Element {
    let st = use_app();
    let store = use_hook(|| Rc::new(RefCell::new(Store::default())));
    use_asset_handler("avatars", move |req: AssetRequest, responder| {
        let Some(descriptor) = descriptor_of(&req) else {
            return respond(responder, None);
        };
        let mut s = store.borrow_mut();
        if let Some(image) = s.images.get(&descriptor) {
            return respond(responder, image.as_deref());
        }
        if let Some(waiting) = s.waiting.get_mut(&descriptor) {
            waiting.push(responder);
            return;
        }
        s.waiting.insert(descriptor.clone(), vec![responder]);
        drop(s);
        let store = store.clone();
        spawn(async move {
            let image = fetch(st, &descriptor).await.map(Rc::new);
            let mut s = store.borrow_mut();
            s.images.insert(descriptor.clone(), image.clone());
            for r in s.waiting.remove(&descriptor).unwrap_or_default() {
                respond(r, image.as_deref());
            }
        });
    });
    rsx! {}
}

fn descriptor_of(req: &AssetRequest) -> Option<String> {
    url::form_urlencoded::parse(req.uri().query()?.as_bytes())
        .find(|(k, _)| k == "d")
        .map(|(_, v)| v.into_owned())
        .filter(|d| !d.is_empty())
}

/// Disk cache, else ADO (saved to the disk cache when caching is on).
async fn fetch(st: AppState, descriptor: &str) -> Option<Vec<u8>> {
    let cache = st.settings.peek().cache_enabled;
    if cache && let Some(bytes) = avatars::load(descriptor) {
        return Some(bytes);
    }
    let client = st.client.peek().clone()?;
    match client.member_avatar(descriptor).await {
        Ok(Some(bytes)) => {
            if cache && let Err(e) = avatars::save(descriptor, &bytes) {
                tracing::debug!(error = %e, "couldn't cache avatar");
            }
            Some(bytes)
        }
        // No photo: initials.
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(error = %e, "couldn't load an avatar (the PAT needs Graph (Read))");
            None
        }
    }
}

fn respond(responder: RequestAsyncResponder, image: Option<&Vec<u8>>) {
    let response = match image {
        Some(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, avatars::content_type(bytes))
            .header(header::CACHE_CONTROL, "max-age=86400")
            .body(bytes.clone()),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Vec::new()),
    };
    responder.respond(response.unwrap_or_else(|_| Response::new(Vec::new())));
}
