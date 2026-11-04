//! The app's one tooltip. Give any element a `data-tip` attribute and hovering it (or focusing it
//! with the keyboard) shows the text in a bubble above it, or below when there is no room. The
//! bubble is a single fixed-position element mounted at the root, so `overflow: hidden` and
//! transforms on the hovered element (ellipsized chips, rotated timeline diamonds) don't affect
//! it. Use `data-tip` instead of `title`, which shows the OS tooltip.

use dioxus::prelude::*;

/// Event delegation on the document: positions `#tip` over the nearest `[data-tip]` ancestor of
/// whatever is hovered or keyboard-focused, and hides it on press, drag, scroll and leave.
const SCRIPT: &str = r#"
(() => {
  if (window.__tipInstalled) return;
  window.__tipInstalled = true;
  const tip = document.getElementById('tip');
  if (!tip) return;
  let anchor = null;
  const hide = () => { anchor = null; tip.classList.remove('on'); };
  const show = (el) => {
    const text = el.getAttribute('data-tip');
    if (!text) return hide();
    anchor = el;
    tip.textContent = text;
    const r = el.getBoundingClientRect();
    const t = tip.getBoundingClientRect();
    const gap = 6;
    let top = r.top - t.height - gap;
    const below = top < 4;
    if (below) top = r.bottom + gap;
    const left = Math.max(4, Math.min(r.left + r.width / 2 - t.width / 2, innerWidth - t.width - 4));
    tip.style.left = left + 'px';
    tip.style.top = top + 'px';
    tip.classList.toggle('below', below);
    tip.classList.add('on');
  };
  const target = (e) => e.target instanceof Element ? e.target.closest('[data-tip]') : null;
  document.addEventListener('mouseover', (e) => {
    const el = target(e);
    if (el !== anchor) el ? show(el) : hide();
  });
  // Pointer left the window.
  document.addEventListener('mouseout', (e) => { if (!e.relatedTarget) hide(); });
  document.addEventListener('focusin', (e) => {
    const el = target(e);
    if (el && e.target.matches(':focus-visible')) show(el);
  });
  document.addEventListener('focusout', hide);
  for (const ev of ['pointerdown', 'dragstart', 'scroll', 'wheel', 'keydown']) {
    document.addEventListener(ev, hide, true);
  }
})();
"#;

#[component]
pub fn TooltipLayer() -> Element {
    use_effect(|| {
        document::eval(SCRIPT);
    });
    rsx! {
        div { id: "tip", class: "tip-bubble", role: "tooltip", aria_hidden: "true" }
    }
}
