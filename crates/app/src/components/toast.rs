//! Toast stack (bottom right): move warnings, conflicts, ADO errors verbatim.

use dioxus::prelude::*;

use crate::components::icons;
use crate::components::widgets::Icon;
use crate::state::{ToastKind, use_app};

#[component]
pub fn Toasts() -> Element {
    let mut st = use_app();
    let toasts = st.toasts.read().clone();
    rsx! {
        div { class: "toasts", role: "status", aria_live: "polite",
            for t in toasts {
                div {
                    key: "{t.id}",
                    class: match t.kind {
                        ToastKind::Info => "toast",
                        ToastKind::Warn => "toast warn",
                        ToastKind::Error => "toast error",
                    },
                    Icon { svg: if t.kind == ToastKind::Info { icons::INFO } else { icons::ALERT } }
                    span { "{t.text}" }
                    button {
                        r#type: "button",
                        aria_label: "Dismiss",
                        onclick: move |_| st.toasts.write().retain(|x| x.id != t.id),
                        Icon { svg: icons::CLOSE }
                    }
                }
            }
        }
    }
}
