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
    return el.dispatchEvent(
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

  // --- contenteditable / ProseMirror ------------------------------------
  //
  // Rich editors (ProseMirror, Slate, Lexical, TipTap) keep their own document
  // model and treat the DOM as a rendering target. Assigning `textContent`
  // either does nothing or is reverted on the next redraw, and the editor's
  // state never learns about the text.
  //
  // What they *do* listen to is the input pipeline: a cancellable
  // `beforeinput` carrying `inputType: "insertText"`, followed by an actual
  // document mutation. `document.execCommand("insertText")` produces exactly
  // that sequence natively — the browser fires the events and edits the
  // selection — which is why it is the primary path here.

  function placeCaret(el, atEnd) {
    var range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(!atEnd);
    var sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
  }

  function selectAllWithin(el) {
    var range = document.createRange();
    range.selectNodeContents(el);
    var sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
  }

  function clearEditable(el) {
    selectAllWithin(el);
    // Ask the editor to delete its own selection, so its model stays in sync.
    var beforeInput = new InputEvent("beforeinput", {
      bubbles: true,
      cancelable: true,
      inputType: "deleteContentBackward",
    });
    if (el.dispatchEvent(beforeInput)) {
      try {
        document.execCommand("delete", false, null);
      } catch (e) {
        el.textContent = "";
      }
    }
    placeCaret(el, true);
  }

  function insertChar(el, ch) {
    // Primary path: the browser fires beforeinput/input and mutates the
    // selection itself, which every rich editor understands.
    var inserted = false;
    try {
      inserted = document.execCommand("insertText", false, ch);
    } catch (e) {
      inserted = false;
    }
    if (inserted) return;

    // Fallback for editors that cancel execCommand: announce the intent, and
    // if nothing handled it, write the character and report it ourselves.
    var before = new InputEvent("beforeinput", {
      bubbles: true,
      cancelable: true,
      inputType: "insertText",
      data: ch,
    });
    var proceed = el.dispatchEvent(before);
    if (proceed) {
      var sel = window.getSelection();
      if (sel && sel.rangeCount) {
        var range = sel.getRangeAt(0);
        range.deleteContents();
        var node = document.createTextNode(ch);
        range.insertNode(node);
        range.setStartAfter(node);
        range.collapse(true);
        sel.removeAllRanges();
        sel.addRange(range);
      } else {
        el.textContent += ch;
      }
    }
    el.dispatchEvent(
      new InputEvent("input", { bubbles: true, data: ch, inputType: "insertText" })
    );
  }

  async function typeIntoEditable(el, text, delay) {
    el.focus();
    placeCaret(el, true);
    clearEditable(el);

    for (var i = 0; i < text.length; i++) {
      var ch = text[i];
      key(el, "keydown", ch);
      insertChar(el, ch);
      key(el, "keyup", ch);
      if (delay > 0) await sleep(delay);
    }
  }

  async function typeIntoField(el, text, delay) {
    el.focus();
    setNativeValue(el, "");

    var acc = "";
    for (var i = 0; i < text.length; i++) {
      var ch = text[i];
      key(el, "keydown", ch);
      acc += ch;
      setNativeValue(el, acc);
      el.dispatchEvent(
        new InputEvent("input", { bubbles: true, data: ch, inputType: "insertText" })
      );
      key(el, "keyup", ch);
      if (delay > 0) await sleep(delay);
    }
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  async function typeInto(el, text, delay) {
    if (el.isContentEditable) return typeIntoEditable(el, text, delay);
    return typeIntoField(el, text, delay);
  }

  function submit(el) {
    // Rich editors bind Enter in a keydown handler and call preventDefault, so
    // keydown is what actually submits. `cancelled` tells us the editor claimed
    // the key — in that case forcing the surrounding form would double-submit.
    var cancelled = !key(el, "keydown", "Enter");
    key(el, "keypress", "Enter");
    key(el, "keyup", "Enter");

    if (cancelled || el.isContentEditable) return;

    var form = el.closest ? el.closest("form") : null;
    if (form && typeof form.requestSubmit === "function") form.requestSubmit();
  }

  // An SSO hop lands the webview on an identity provider - Google, Okta, Azure.
  // Those pages hold the user's real password field, and the probe selectors
  // are meaningless there. Acting on a host we were not pointed at could type
  // into a credential form, so every entry point checks this first.
  var onExpectedHost = function (p) {
    if (!p.expected_host) return true;
    return location.hostname === p.expected_host;
  };

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
      if (!onExpectedHost(p)) {
        invoke({
          code: 401,
          detail: "redirected to " + location.hostname + " (sign-in required)",
          latency_ms: 0,
          nonce: p.nonce,
        });
        return;
      }
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
        if (!onExpectedHost(p)) {
          return done(401, "redirected to " + location.hostname + " (sign-in required)");
        }
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
