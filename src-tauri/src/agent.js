// Pong synthetic probe agent.
//
// Installed into the hidden webview via `initialization_script`, so it re-runs
// on every navigation, before the page's own scripts parse. It never touches
// the page until Rust explicitly calls `runCheck` / `heartbeat`.
(function () {
  if (window.__PONG__) return;

  // The IPC bridge is the only way back to Rust, so a failure here cannot be
  // reported through it. Leave a breadcrumb in the page title instead — Rust
  // logs the title whenever a probe times out.
  var breadcrumb = function (why) {
    try {
      document.title = "PONG_IPC_ERROR: " + why;
    } catch (e) {}
  };

  var invoke = function (payload) {
    try {
      var pending = window.__TAURI_INTERNALS__.invoke("report_health", {
        payload: payload,
      });
      // `invoke` rejects asynchronously (e.g. when the ACL denies the command),
      // so a synchronous try/catch alone would swallow the failure silently.
      if (pending && typeof pending.catch === "function") {
        pending.catch(function (err) {
          breadcrumb(String((err && err.message) || err));
        });
      }
    } catch (e) {
      breadcrumb(String((e && e.message) || e));
    }
  };

  var sleep = function (ms) {
    return new Promise(function (r) {
      setTimeout(r, ms);
    });
  };

  // Selectors come from user config, so a typo must not throw.
  var q = function (sel) {
    if (!sel) return null;
    try {
      return document.querySelector(sel);
    } catch (e) {
      return null;
    }
  };

  // React/Vue cache the input value on their own state: a plain `el.value = x`
  // is silently reverted. Going through the prototype's native setter is what
  // makes the framework observe the change.
  function setNativeValue(el, value) {
    var desc = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value");
    if (desc && desc.set) desc.set.call(el, value);
    else el.value = value;
  }

  function key(el, type, k) {
    el.dispatchEvent(
      new KeyboardEvent(type, {
        key: k,
        code: k === "Enter" ? "Enter" : "Key" + k.toUpperCase(),
        keyCode: k === "Enter" ? 13 : k.charCodeAt(0),
        which: k === "Enter" ? 13 : k.charCodeAt(0),
        bubbles: true,
        cancelable: true,
      })
    );
  }

  async function typeInto(el, text, delay) {
    el.focus();
    var editable = el.isContentEditable;
    if (editable) el.textContent = "";
    else setNativeValue(el, "");

    var acc = "";
    for (var i = 0; i < text.length; i++) {
      var ch = text[i];
      key(el, "keydown", ch);
      acc += ch;
      if (editable) el.textContent = acc;
      else setNativeValue(el, acc);
      el.dispatchEvent(
        new InputEvent("input", { bubbles: true, data: ch, inputType: "insertText" })
      );
      key(el, "keyup", ch);
      if (delay > 0) await sleep(delay);
    }
    if (!editable) el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  function submit(el) {
    key(el, "keydown", "Enter");
    key(el, "keypress", "Enter");
    key(el, "keyup", "Enter");
    var form = el.closest ? el.closest("form") : null;
    if (form && typeof form.requestSubmit === "function") form.requestSubmit();
  }

  window.__PONG__ = {
    // Read-only DOM inspection: which of the two markers is on screen?
    probe: function (p) {
      if (q(p.selectors.login_indicator)) return { code: 401, detail: "login screen detected" };
      if (q(p.selectors.authenticated)) return { code: 200, detail: "authenticated" };
      return { code: 503, detail: "neither auth nor login marker found" };
    },

    // Cheap status question asked before committing to a full check.
    heartbeat: function (p) {
      var started = performance.now();
      var r = this.probe(p);
      invoke({
        code: r.code,
        detail: r.detail,
        latency_ms: Math.round(performance.now() - started),
        nonce: p.nonce,
      });
    },

    // Full synthetic interaction: click, type, submit, wait, re-probe.
    runCheck: async function (p) {
      var started = performance.now();
      var self = this;
      var done = function (code, detail) {
        invoke({
          code: code,
          detail: detail,
          latency_ms: Math.round(performance.now() - started),
          nonce: p.nonce,
        });
      };

      try {
        if (document.readyState === "loading") {
          await new Promise(function (r) {
            document.addEventListener("DOMContentLoaded", r, { once: true });
          });
        }

        var pre = self.probe(p);
        if (pre.code !== 200) return done(pre.code, pre.detail);

        if (p.selectors.action_button) {
          var btn = q(p.selectors.action_button);
          if (btn) {
            btn.click();
            await sleep(400);
          }
        }

        var input = q(p.selectors.text_input);
        if (!input) return done(503, "text input not found: " + p.selectors.text_input);

        await typeInto(input, p.payload, p.typing_delay_ms);
        submit(input);
        await sleep(p.settle_ms);

        var post = self.probe(p);
        if (post.code === 401) return done(401, "session expired during check");
        if (post.code !== 200) return done(503, post.detail);
        return done(200, "dashboard responded");
      } catch (err) {
        return done(500, String((err && err.message) || err));
      }
    },
  };
})();
