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

  var invokeCommand = function (command, payload) {
    try {
      var pending = window.__TAURI_INTERNALS__.invoke(command, {
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

  var invoke = function (payload) {
    invokeCommand("report_health", payload);
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

  // --- resilient element lookup -----------------------------------------
  //
  // A React SPA mounts asynchronously. Querying once and giving up reports a
  // healthy dashboard as broken purely because the check arrived first — a
  // false negative, which is the worst failure mode for a monitor.
  //
  // "Interactive" means more than present: a submit button typically renders
  // disabled and only enables once the editor holds content, so waiting for
  // existence alone would click a dead button.

  function isInteractive(el) {
    if (!el) return false;
    if (el.disabled === true) return false;
    if (el.getAttribute && el.getAttribute("aria-disabled") === "true") return false;
    // Zero-area elements are still in the tree but cannot be clicked.
    var box = el.getBoundingClientRect ? el.getBoundingClientRect() : null;
    if (box && box.width === 0 && box.height === 0) return false;
    return true;
  }

  // A `setTimeout` poll loop is exactly the kind of callback macOS/Chromium
  // throttle hardest on an occluded webview — Pong's hidden window by design
  // — which can silently stretch a 100ms poll into 1s+ and blow the check's
  // time budget. `MutationObserver` fires on DOM changes rather than a timer,
  // so it keeps working at full speed regardless of visibility; only the
  // final "give up" deadline still needs a single timer, and a coarse delay
  // on that one is harmless.
  function waitForElement(selector, timeoutMs, requireInteractive) {
    return new Promise(function (resolve) {
      var settled = false;
      var finish = function (el) {
        if (settled) return;
        settled = true;
        observer.disconnect();
        clearTimeout(deadlineTimer);
        resolve(el);
      };
      var check = function () {
        var el = q(selector);
        if (el && (!requireInteractive || isInteractive(el))) finish(el);
      };

      var observer = new MutationObserver(check);
      observer.observe(document.documentElement || document.body, {
        childList: true,
        subtree: true,
        attributes: true,
        characterData: true,
      });
      var deadlineTimer = setTimeout(function () {
        finish(null);
      }, timeoutMs || 10000);

      check();
    });
  }

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

  // --- reading the reply back out -----------------------------------------
  //
  // A streaming reply keeps mutating its own textContent for as long as it is
  // generating. Waiting a fixed settle_ms and grabbing whatever is there risks
  // capturing a half-written sentence. Instead, poll the *last* matching
  // element and treat the text as final once it stops changing for a short
  // stability window — cheap, and it does not depend on knowing the
  // dashboard's own "generating" indicator.

  function lastMatchText(selector) {
    var nodes = q_all(selector);
    if (!nodes.length) return null;
    var el = nodes[nodes.length - 1];
    return el.textContent || "";
  }

  function q_all(selector) {
    if (!selector) return [];
    try {
      return Array.prototype.slice.call(document.querySelectorAll(selector));
    } catch (e) {
      return [];
    }
  }

  // Same throttling hazard as waitForElement: a tight setTimeout poll loop
  // stalls hard on an occluded webview. MutationObserver reacts to the reply
  // actually changing, so streaming keeps this responsive regardless of
  // visibility; only the "stable for 800ms" and final deadline checks use a
  // timer, and each is a single one-shot rather than a loop.
  function waitForStableText(selector, timeoutMs) {
    return new Promise(function (resolve) {
      var settled = false;
      var last = null;
      var stableTimer = null;

      var finish = function () {
        if (settled) return;
        settled = true;
        observer.disconnect();
        clearTimeout(stableTimer);
        clearTimeout(deadlineTimer);
        resolve(last ? last.trim() : null);
      };

      var check = function () {
        var text = lastMatchText(selector);
        if (text && text.trim() && text !== last) {
          last = text;
          clearTimeout(stableTimer);
          stableTimer = setTimeout(finish, 800);
        }
      };

      var observer = new MutationObserver(check);
      observer.observe(document.documentElement || document.body, {
        childList: true,
        subtree: true,
        characterData: true,
      });
      var deadlineTimer = setTimeout(finish, timeoutMs || 10000);

      check();
    });
  }

  // --- post-check teardown -------------------------------------------------
  //
  // Deletes whatever the check just created, so a monitor running every few
  // minutes does not silently fill the dashboard with check artifacts
  // forever. Each step only runs if configured, and every failure is
  // reported back through the check's own detail rather than swallowed —
  // "the check succeeded but cleanup didn't" is a distinct, worth-knowing
  // outcome, not a check failure.
  // One-shot diagnostic dump, used only in the failure message so a stuck
  // cleanup step is debuggable from the History detail alone, without a
  // separate DevTools session.
  function diagnoseDialog() {
    try {
      var dialogs = document.querySelectorAll('[role="dialog"], [role="alertdialog"]');
      var buttons = document.querySelectorAll(
        '[role="dialog"] button, [role="alertdialog"] button'
      );
      var buttonTexts = Array.prototype.slice
        .call(buttons)
        .map(function (b) {
          return '"' + (b.textContent || "").trim().slice(0, 20) + '"';
        })
        .join(",");
      return (
        "dialogs=" +
        dialogs.length +
        " buttons=" +
        buttons.length +
        " texts=[" +
        buttonTexts +
        "]"
      );
    } catch (e) {
      return "diagnose failed: " + String((e && e.message) || e);
    }
  }

  async function runCleanup(cleanup, timeoutMs) {
    try {
      if (cleanup.menu_button) {
        var menu = await waitForElement(cleanup.menu_button, timeoutMs, true);
        if (!menu) return "cleanup failed: menu button never appeared";
        menu.click();
      }
      if (cleanup.delete_option) {
        var del = await waitForElement(cleanup.delete_option, timeoutMs, true);
        if (!del) return "cleanup failed: delete option never appeared";
        del.click();
      }
      if (cleanup.confirm_button) {
        var confirmBtn = await waitForElement(cleanup.confirm_button, timeoutMs, true);
        if (!confirmBtn) {
          return "cleanup failed: confirm button never appeared (" + diagnoseDialog() + ")";
        }
        confirmBtn.click();
      }
      return "cleanup: ok";
    } catch (err) {
      return "cleanup failed: " + String((err && err.message) || err);
    }
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

    // Full synthetic interaction. Every DOM lookup goes through waitForElement,
    // because in a single-page app the check routinely arrives before React has
    // finished mounting — and reporting that as a failure is a false negative.
    runCheck: async function (p) {
      var started = performance.now();
      var self = this;
      var timeout = p.element_timeout_ms || 10000;
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

        // 1. Optional entry point (e.g. "new conversation").
        if (p.selectors.action_button) {
          var btn = await waitForElement(p.selectors.action_button, timeout, true);
          if (!btn) {
            return done(503, "action button never became clickable: " + p.selectors.action_button);
          }
          btn.click();
        }

        // 2. The editor itself.
        var input = await waitForElement(p.selectors.text_input, timeout, true);
        if (!input) {
          return done(503, "text input never appeared: " + p.selectors.text_input);
        }

        // 3. Type the payload the way a person would.
        await typeInto(input, p.payload, p.typing_delay_ms);

        // 4. Submit. A React form usually keeps its button disabled until the
        //    editor reports content, so waiting for it to *enable* is also a
        //    check that the typing actually reached the app's state.
        if (p.selectors.submit_button) {
          var submitBtn = await waitForElement(p.selectors.submit_button, timeout, true);
          if (!submitBtn) {
            return done(
              503,
              "submit button never enabled — the editor likely never registered the input"
            );
          }
          submitBtn.click();
        } else {
          submit(input);
        }

        // 5. Let the dashboard react, then confirm we are still authenticated.
        await sleep(p.settle_ms);

        var post = self.probe(p);
        if (post.code === 401) return done(401, "session expired during check");
        if (post.code !== 200) return done(503, post.detail);

        var detail = "dashboard responded";
        if (p.selectors.response) {
          var remaining = timeout - (performance.now() - started);
          var reply = await waitForStableText(p.selectors.response, remaining);
          detail = reply
            ? reply.length > 300
              ? reply.slice(0, 300) + "…"
              : reply
            : "dashboard responded (no reply captured within timeout)";
        }

        if (p.cleanup && (p.cleanup.menu_button || p.cleanup.delete_option || p.cleanup.confirm_button)) {
          var remainingForCleanup = timeout - (performance.now() - started);
          detail += " · " + (await runCleanup(p.cleanup, remainingForCleanup));
        }

        return done(200, detail);
      } catch (err) {
        return done(500, String((err && err.message) || err));
      }
    },

    // Reads claude.ai's usage-limits panel (session %, weekly %, reset
    // countdowns). Hardcoded to that page's current DOM, not driven by
    // config — unlike everything else here, this is not a generic dashboard
    // check. Two things make it locale-proof, since the account's language
    // changes the wording ("Resets in 3 hr 43 min" vs "Reinicia em 3 h 48
    // min"): percentages are found via the universal "%" character rather
    // than any word, and "reset" text is identified by CSS class, not by
    // matching "Resets"/"Reinicia".
    scrapeUsage: function (p) {
      var result = { session_percent: null, session_reset_text: null, weekly_percent: null, weekly_reset_text: null };
      try {
        var allSpans = Array.prototype.slice.call(document.querySelectorAll("span"));

        var percentSpans = allSpans.filter(function (el) {
          var text = (el.textContent || "").trim();
          return text.length > 0 && text.length < 20 && text.indexOf("%") !== -1;
        });

        // The reset-countdown spans share the same "footnote/secondary" text
        // style as the percentage spans, minus the percentage-only sizing
        // class — and, unlike the percentage spans, never contain "%".
        var resetSpans = allSpans.filter(function (el) {
          var cls = el.className;
          if (typeof cls !== "string") return false;
          if (cls.indexOf("text-footnote") === -1 || cls.indexOf("text-secondary") === -1) return false;
          if (cls.indexOf("min-w-") !== -1) return false;
          var text = (el.textContent || "").trim();
          return text.length > 0 && text.indexOf("%") === -1;
        });

        var percentOf = function (el) {
          if (!el) return null;
          var m = /(\d+)\s*%/.exec(el.textContent || "");
          return m ? Number(m[1]) : null;
        };

        // Row order on the page is session first, then the weekly ("all
        // models") row — both lists follow that same order.
        result.session_percent = percentOf(percentSpans[0]);
        result.session_reset_text = resetSpans[0] ? resetSpans[0].textContent.trim() : null;
        result.weekly_percent = percentOf(percentSpans[1]);
        result.weekly_reset_text = resetSpans[1] ? resetSpans[1].textContent.trim() : null;
      } catch (e) {
        breadcrumb(String((e && e.message) || e));
      }

      invokeCommand("report_usage", {
        session_percent: result.session_percent,
        session_reset_text: result.session_reset_text,
        weekly_percent: result.weekly_percent,
        weekly_reset_text: result.weekly_reset_text,
        nonce: p.nonce,
      });
    },
  };
})();
