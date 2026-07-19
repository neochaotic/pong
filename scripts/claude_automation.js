import { chromium } from 'playwright';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const userDataDir = path.join(__dirname, 'claude_user_data');

/**
 * Opens your system's Google Chrome browser for login.
 * The session is kept inside the `claude_user_data` directory.
 */
async function login() {
  console.log('========================================================');
  console.log('OPENING CHROME FOR LOGIN...');
  console.log('Using your system\'s Google Chrome to bypass Cloudflare.');
  console.log('1. Please log in to your Claude.ai account in the browser window.');
  console.log('2. Click the Cloudflare "Verify you are human" checkbox if it shows up.');
  console.log('3. Once you log in and see the chat page, close Chrome.');
  console.log('========================================================');

  // Launching a persistent context using your actual Google Chrome
  const context = await chromium.launchPersistentContext(userDataDir, {
    headless: false,
    channel: 'chrome', // Use installed Google Chrome to look like a normal browser
    ignoreDefaultArgs: ['--enable-automation'], // Removes the automation warning banner
    args: [
      '--disable-blink-features=AutomationControlled', // Hides navigator.webdriver (avoids bot detection)
    ],
  });

  const page = await context.newPage();
  await page.goto('https://claude.ai/new');

  // Keep the script running until the browser window/context is closed by you
  await new Promise((resolve) => {
    context.on('close', resolve);
  });

  console.log(`\nSession profile successfully updated in: ${userDataDir}`);
}

/**
 * Sends a message using the saved profile session.
 */
async function sendMessage(message) {
  console.log(`Sending message: "${message}"`);

  // Launch with the same persistent context to load cookies/session
  const context = await chromium.launchPersistentContext(userDataDir, {
    headless: false, // Set to true if you want to run it in the background later
    channel: 'chrome',
    ignoreDefaultArgs: ['--enable-automation'],
    args: [
      '--disable-blink-features=AutomationControlled',
    ],
  });

  try {
    const page = await context.newPage();
    await page.goto('https://claude.ai/new');

    console.log('Waiting for the chat input field...');
    const chatInput = page.locator('div[contenteditable="true"]').first();
    await chatInput.waitFor({ state: 'visible', timeout: 30000 });

    // Focus and fill the message
    await chatInput.click();
    await chatInput.fill(message);
    console.log('Message typed.');

    // Click the send button
    const sendButton = page.locator('button[aria-label*="Send"], button[aria-label*="Enviar"], button:has(svg)').first();
    await sendButton.waitFor({ state: 'visible', timeout: 10000 });
    
    await sendButton.click();
    console.log('Send button clicked!');

    // Wait to see response generating
    console.log('Waiting for Claude\'s response...');
    await page.waitForTimeout(8000);

  } catch (e) {
    console.error('An error occurred during automation:', e);
  } finally {
    await context.close();
    console.log('Chrome closed.');
  }
}

/**
 * Dumps every element carrying a data-testid, so we can eyeball which one is
 * stable enough to use as a Pong selector (generated CSS classes change on
 * every deploy; data-testid usually does not).
 */
async function dumpTestIds(page, label, minTextLength) {
  const items = await page.evaluate((minLen) => {
    return Array.from(document.querySelectorAll('[data-testid]'))
      .map((el) => ({
        testid: el.getAttribute('data-testid'),
        tag: el.tagName.toLowerCase(),
        textLength: (el.textContent || '').trim().length,
        text: (el.textContent || '').trim().slice(0, 80),
      }))
      .filter((c) => c.textLength >= minLen);
  }, minTextLength || 0);

  console.log(`\n=== data-testid elements (${label}) ===`);
  if (items.length === 0) {
    console.log('(none found — this dashboard may not use data-testid; look for stable aria-label/id instead)');
  }
  for (const item of items) {
    console.log(`[data-testid="${item.testid}"]  <${item.tag}>  (${item.textLength} chars)  "${item.text}"`);
  }
}

/**
 * Dumps data-testid elements from the authenticated session — candidates for
 * selectors.authenticated.
 */
async function inspectAuthenticated() {
  const context = await chromium.launchPersistentContext(userDataDir, {
    headless: false,
    channel: 'chrome',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  });
  try {
    const page = await context.newPage();
    await page.goto('https://claude.ai/new');
    await page.waitForTimeout(3000);
    await dumpTestIds(page, 'logged in — pick one that is ONLY here', 0);
  } finally {
    await context.close();
  }
}

/**
 * Opens a brand-new, non-persistent Chrome profile (no saved cookies), so the
 * page renders logged out — candidates for selectors.login_indicator.
 */
async function inspectLoggedOut() {
  const browser = await chromium.launch({ headless: false, channel: 'chrome' });
  try {
    const page = await browser.newPage();
    await page.goto('https://claude.ai/new');
    await page.waitForTimeout(4000);

    console.log(`\nURL after navigation: ${page.url()}`);
    console.log(`Page title: ${await page.title()}`);

    await dumpTestIds(page, 'logged out — pick one that is ONLY here', 0);

    // The login screen is often a different component tree than the app
    // (a Cloudflare check, a dedicated auth page) and may carry no
    // data-testid at all. Fall back to inputs/buttons, which a login form
    // always has.
    const fields = await page.evaluate(() =>
      Array.from(document.querySelectorAll('input, button')).map((el) => ({
        tag: el.tagName.toLowerCase(),
        type: el.getAttribute('type') || '',
        id: el.id || '',
        ariaLabel: el.getAttribute('aria-label') || '',
        placeholder: el.getAttribute('placeholder') || '',
        text: (el.textContent || '').trim().slice(0, 40),
      }))
    );
    console.log('\n=== inputs / buttons on the login screen ===');
    if (fields.length === 0) {
      console.log('(none — page may still be loading; rerun, or check the Chrome window manually)');
    }
    for (const f of fields) {
      console.log(
        `<${f.tag}${f.type ? ' type=' + f.type : ''}${f.id ? ' id=' + f.id : ''}${
          f.ariaLabel ? ' aria-label="' + f.ariaLabel + '"' : ''
        }${f.placeholder ? ' placeholder="' + f.placeholder + '"' : ''}>  "${f.text}"`
      );
    }
  } finally {
    await browser.close();
  }
}

/**
 * Sends a message and dumps data-testid elements with real text content
 * afterward — the reply bubble should stand out among a short list.
 */
async function inspectResponse(message) {
  const context = await chromium.launchPersistentContext(userDataDir, {
    headless: false,
    channel: 'chrome',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  });
  try {
    const page = await context.newPage();
    await page.goto('https://claude.ai/new');

    const chatInput = page.locator('div[contenteditable="true"]').first();
    await chatInput.waitFor({ state: 'visible', timeout: 30000 });
    await chatInput.click();
    await chatInput.fill(message);

    const sendButton = page
      .locator('button[aria-label*="Send"], button[aria-label*="Enviar"]')
      .first();
    await sendButton.click();

    console.log('Waiting for navigation off /new (a real chat was created)...');
    await page
      .waitForURL((url) => !url.pathname.endsWith('/new'), { timeout: 15000 })
      .catch(() => console.log('(URL never left /new — the send may not have registered)'));
    console.log('Now at:', page.url());

    console.log('Waiting for the reply text to stop growing...');
    let last = '';
    let stableRounds = 0;
    for (let i = 0; i < 30; i++) {
      const len = await page.evaluate(() => document.body.innerText.length);
      if (String(len) === last) {
        stableRounds++;
        if (stableRounds >= 3) break;
      } else {
        stableRounds = 0;
      }
      last = String(len);
      await page.waitForTimeout(1000);
    }

    // A 20-char floor filters out buttons/icons and keeps prose-sized blocks,
    // which is where the reply bubble almost certainly lives.
    await dumpTestIds(page, 'after a reply — the reply bubble is usually the longest one', 20);

    // Assistant messages often carry no data-testid of their own; fall back
    // to scanning by class name, which is where a Tailwind-style chat UI
    // usually marks "this is a message" even without a testid.
    const byClass = await page.evaluate(() => {
      const seen = new Set();
      const out = [];
      document.querySelectorAll('[class]').forEach((el) => {
        const text = (el.textContent || '').trim();
        if (text.length < 20 || text.length > 4000) return;
        // Skip ancestors of things we already captured — keep the innermost
        // element that still holds the full text.
        const hasTextChild = Array.from(el.children).some(
          (c) => (c.textContent || '').trim().length === text.length
        );
        if (hasTextChild) return;
        if (seen.has(text)) return;
        seen.add(text);
        out.push({
          tag: el.tagName.toLowerCase(),
          className: String(el.className).slice(0, 80),
          testid: el.getAttribute('data-testid') || '',
          text: text.slice(0, 80),
        });
      });
      return out.slice(0, 15);
    });
    console.log('\n=== innermost text-bearing elements (fallback, by class) ===');
    for (const b of byClass) {
      console.log(
        `<${b.tag} class="${b.className}"${b.testid ? ' data-testid="' + b.testid + '"' : ''}>  "${b.text}"`
      );
    }
  } finally {
    await context.close();
  }
}

/**
 * Opens a URL with the saved session and leaves Chrome open — for manual
 * DevTools inspection. Does not close the context; kill the process (or just
 * close the Chrome window) when done.
 */
async function open(url) {
  const context = await chromium.launchPersistentContext(userDataDir, {
    headless: false,
    channel: 'chrome',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  });
  const page = await context.newPage();
  await page.goto(url || 'https://claude.ai/new');
  console.log('Chrome is open and left running for manual inspection.');
  await new Promise(() => {});
}

/**
 * Hovers the most recent sidebar chat, opens its options menu, and dumps
 * what's in it — candidates for the delete-conversation cleanup flow
 * (menu_button -> delete_option -> confirm_button).
 */
async function inspectDeleteFlow() {
  const context = await chromium.launchPersistentContext(userDataDir, {
    headless: false,
    channel: 'chrome',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  });
  try {
    const page = await context.newPage();
    await page.goto('https://claude.ai/new');
    await page.waitForTimeout(2500);

    const firstChat = page.locator('a[href^="/chat/"]').first();
    await firstChat.waitFor({ state: 'visible', timeout: 15000 });
    await page.evaluate(() => document.querySelector('a[href^="/chat/"]').click());
    await page.waitForTimeout(2000);
    console.log('Opened conversation at:', page.url());

    // Prefer a control scoped to the OPEN conversation (the page we are
    // definitely on) over a sidebar-list hover-button, which has no built-in
    // guarantee of targeting the conversation we just created rather than
    // whatever happens to be first/hovered in the list.
    console.log('Looking for an options/menu button in the page header...');
    const headerButtons = await page.evaluate(() => {
      const header =
        document.querySelector('[data-testid="page-header"]') || document.querySelector('header');
      if (!header) return { found: false, buttons: [] };
      return {
        found: true,
        buttons: Array.from(header.querySelectorAll('button')).map((b) => ({
          ariaLabel: b.getAttribute('aria-label') || '',
          testid: b.getAttribute('data-testid') || '',
          text: (b.textContent || '').trim(),
        })),
      };
    });
    console.log('header found:', headerButtons.found);
    console.log('\n=== buttons inside the page header ===');
    for (const b of headerButtons.buttons) {
      console.log(`aria-label="${b.ariaLabel}" data-testid="${b.testid}" text="${b.text}"`);
    }

    console.log('\nFalling back to the sidebar hover-button for comparison...');
    const found = await page.evaluate(() => {
      const btn = document.querySelector('button[aria-label^="More options for"]');
      if (!btn) return false;
      btn.click();
      return true;
    });
    await page.waitForTimeout(500);

    await dumpTestIds(page, 'after opening a sidebar item menu', 0);

    const menuItems = await page.evaluate(() =>
      Array.from(document.querySelectorAll('[role="menuitem"], [role="menuitemradio"]')).map(
        (el) => ({
          text: (el.textContent || '').trim(),
          testid: el.getAttribute('data-testid') || '',
        })
      )
    );
    console.log('\n=== [role="menuitem"] elements ===');
    if (menuItems.length === 0) console.log('(none found)');
    for (const m of menuItems) {
      console.log(`"${m.text}"  data-testid="${m.testid}"`);
    }

    const hasDelete = menuItems.some((m) => m.testid === 'delete-chat-trigger');
    if (hasDelete) {
      console.log('\nClicking [data-testid="delete-chat-trigger"] to see the confirm dialog...');
      await page.evaluate(() => {
        document.querySelector('[data-testid="delete-chat-trigger"]').click();
      });
      await page.waitForTimeout(800);
      await dumpTestIds(page, 'after clicking delete — confirm dialog', 0);

      const dialogButtons = await page.evaluate(() =>
        Array.from(document.querySelectorAll('[role="dialog"] button, [role="alertdialog"] button')).map(
          (b) => ({
            text: (b.textContent || '').trim(),
            testid: b.getAttribute('data-testid') || '',
            ariaLabel: b.getAttribute('aria-label') || '',
            outerHTML: b.outerHTML.slice(0, 500),
          })
        )
      );
      console.log('\n=== buttons inside the confirm dialog (full attributes) ===');
      if (dialogButtons.length === 0) console.log('(none found — maybe deletion has no confirm step)');
      for (const b of dialogButtons) {
        console.log(`"${b.text}"\n${b.outerHTML}\n`);
      }
    }
  } finally {
    await context.close();
  }
}

/**
 * Dumps the usage-limits panel (session %, weekly %, reset countdowns) —
 * candidates for a future usage-dashboard view in Pong.
 */
async function inspectUsage() {
  const context = await chromium.launchPersistentContext(userDataDir, {
    headless: false,
    channel: 'chrome',
    ignoreDefaultArgs: ['--enable-automation'],
    args: ['--disable-blink-features=AutomationControlled'],
  });
  try {
    const page = await context.newPage();
    await page.goto('https://claude.ai/settings/usage');
    await page.waitForTimeout(3000);
    console.log('URL:', page.url());

    await dumpTestIds(page, 'usage settings page', 0);

    // Fall back to a broad text-bearing scan, since progress/percentage
    // widgets frequently skip data-testid.
    const candidates = await page.evaluate(() => {
      const out = [];
      document.querySelectorAll('*').forEach((el) => {
        const text = (el.textContent || '').trim();
        if (!text || text.length > 60) return;
        if (!/%|reinicia|reset|hora|min/i.test(text)) return;
        const hasTextChild = Array.from(el.children).some(
          (c) => (c.textContent || '').trim() === text
        );
        if (hasTextChild) return;
        out.push({
          tag: el.tagName.toLowerCase(),
          className: String(el.className).slice(0, 100),
          testid: el.getAttribute('data-testid') || '',
          text,
        });
      });
      return out.slice(0, 40);
    });
    console.log('\n=== elements mentioning %/reinicia/reset/hora/min ===');
    for (const c of candidates) {
      console.log(`<${c.tag} class="${c.className}"${c.testid ? ' data-testid="' + c.testid + '"' : ''}>  "${c.text}"`);
    }
  } finally {
    await context.close();
  }
}

const command = process.argv[2];
const arg = process.argv[3] || 'Hello Claude, this is an automation test!';

if (command === 'login') {
  login();
} else if (command === 'send') {
  sendMessage(arg);
} else if (command === 'inspect') {
  inspectAuthenticated();
} else if (command === 'inspect-logged-out') {
  inspectLoggedOut();
} else if (command === 'inspect-response') {
  inspectResponse(arg);
} else if (command === 'open') {
  open(arg);
} else if (command === 'inspect-delete-flow') {
  inspectDeleteFlow();
} else if (command === 'inspect-usage') {
  inspectUsage();
} else {
  console.log('Usage:');
  console.log('  node scripts/claude_automation.js login                  - Opens Chrome to log in and save session');
  console.log('  node scripts/claude_automation.js send "msg"             - Sends a message using the saved session');
  console.log('  node scripts/claude_automation.js inspect                - Lists data-testid elements while logged in (selectors.authenticated)');
  console.log('  node scripts/claude_automation.js inspect-logged-out     - Lists data-testid elements logged out, fresh profile (selectors.login_indicator)');
  console.log('  node scripts/claude_automation.js inspect-response "msg" - Sends a message and lists data-testid elements with real text (selectors.response)');
}
