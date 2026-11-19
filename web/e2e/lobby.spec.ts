import { type Browser, type Page, expect, test } from '@playwright/test'

// M1 "done" scenario: two browser windows join the same room and see each other in the lobby;
// the host starts the game. Each actor gets its own browser context (separate cookies/storage).

/** Console errors from every page (CSP violations show up here), checked after each test. */
let consoleErrors: string[] = []

test.beforeEach(() => {
  consoleErrors = []
})

test.afterEach(() => {
  // Expected noise: WebSocket closes during teardown, and the browser logging 4xx responses the
  // app handles on purpose (logged-out check = 401, "nickname taken" = 409). Anything else,
  // e.g. a CSP violation ("Refused to load…") or a 5xx, fails the test.
  const real = consoleErrors.filter(
    (e) => !/WebSocket/i.test(e) && !/Failed to load resource: the server responded with a status of 4\d\d/.test(e),
  )
  expect(real, 'unexpected console errors (e.g. CSP violations)').toEqual([])
})

async function newPage(browser: Browser): Promise<Page> {
  const context = await browser.newContext()
  const page = await context.newPage()
  page.on('console', (msg) => {
    if (msg.type() === 'error') consoleErrors.push(msg.text())
  })
  return page
}

async function joinAs(page: Page, code: string, nickname: string) {
  await page.goto('/')
  await page.getByRole('link', { name: 'Join a game' }).click()
  await page.getByLabel('Room code').fill(code.toLowerCase()) // kids type lowercase
  await page.getByLabel('Your nickname').fill(nickname)
  await page.getByRole('button', { name: "Let's go!" }).click()
  await expect(page.getByRole('heading', { name: `You are ${nickname}` })).toBeVisible()
}

test('host creates a game, two players meet in the lobby, host starts it', async ({ browser }) => {
  const host = await newPage(browser)
  const ali = await newPage(browser)
  const bea = await newPage(browser)

  // Host registers and creates a session.
  await host.goto('/host')
  await expect(host).toHaveURL(/\/host\/login$/)
  await host.getByRole('button', { name: 'No account yet? Create one' }).click()
  await host.getByLabel('Your name (shown to you only)').fill('Ms Trabelsi')
  await host.getByLabel('Email').fill(`e2e-${Date.now()}@example.com`)
  await host.getByLabel('Password').fill('password123')
  await host.getByRole('button', { name: 'Create account' }).click()
  await expect(host.getByRole('heading', { name: 'Hello, Ms Trabelsi' })).toBeVisible()
  await host.getByRole('button', { name: 'Create game' }).click()

  const codeEl = host.getByTestId('room-code')
  await expect(codeEl).toHaveText(/^[A-Z2-9]{6}$/)
  const code = (await codeEl.textContent())!.trim()
  const start = host.getByRole('button', { name: 'Start the game' })
  await expect(start).toBeDisabled()

  // Two kids join from their own devices and see each other.
  await joinAs(ali, code, 'Ali')
  await joinAs(bea, code, 'Bea')
  for (const page of [ali, bea, host]) {
    await expect(page.getByTestId('player-slot')).toHaveCount(2)
    await expect(page.getByTestId('player-slot').filter({ hasText: 'Ali' })).toBeVisible()
    await expect(page.getByTestId('player-slot').filter({ hasText: 'Bea' })).toBeVisible()
  }

  // A taken nickname is refused with a friendly message.
  const late = await newPage(browser)
  await late.goto(`/join?code=${code}`)
  await late.getByLabel('Your nickname').fill('ali')
  await late.getByRole('button', { name: "Let's go!" }).click()
  await expect(late.getByRole('alert')).toContainText('already has that nickname')

  // Both get ready; only then can the host start.
  await ali.getByRole('button', { name: "I'm ready!" }).click()
  await expect(start).toBeDisabled()
  await bea.getByRole('button', { name: "I'm ready!" }).click()
  await expect(host.getByText('Everyone is ready!')).toBeVisible()
  await expect(start).toBeEnabled()
  await start.click()

  for (const page of [ali, bea]) {
    await expect(page.getByText('Puzzle 1 of 1')).toBeVisible()
    await expect(page.getByRole('timer')).toHaveText(/^(20:00|19:5\d)$/)
  }
  await expect(host.getByRole('heading', { name: 'Game in progress' })).toBeVisible()

  // Reloading a player's tab keeps them in the game (token survives in sessionStorage).
  await ali.reload()
  await expect(ali.getByText('Puzzle 1 of 1')).toBeVisible()

  // Ali's whole browser closes (token gone). Bea is told to wait...
  await ali.context().close()
  await expect(bea.getByText('Waiting for Ali to come back…')).toBeVisible()

  // ...and Ali gets back in from a fresh browser just by typing the same name.
  const aliAgain = await newPage(browser)
  await joinAs(aliAgain, code, 'ali')
  await expect(aliAgain.getByText('Puzzle 1 of 1')).toBeVisible()
  await expect(aliAgain.getByRole('heading', { name: 'You are Ali' })).toBeVisible()
  await expect(bea.getByText('Waiting for Ali to come back…')).toBeHidden()
})

async function hostCreatesGame(browser: Browser, name: string): Promise<{ host: Page; code: string }> {
  const host = await newPage(browser)
  await host.goto('/host/login')
  await host.getByRole('button', { name: 'No account yet? Create one' }).click()
  await host.getByLabel('Your name (shown to you only)').fill(name)
  await host.getByLabel('Email').fill(`e2e-${name.replace(/\W/g, '')}-${Date.now()}@example.com`)
  await host.getByLabel('Password').fill('password123')
  await host.getByRole('button', { name: 'Create account' }).click()
  await host.getByRole('button', { name: 'Create game' }).click()
  const code = (await host.getByTestId('room-code').textContent())!.trim()
  return { host, code }
}

/** The kids "talking": the manual reader reads each rule, the lock holder reports matching tiles. */
async function workOutCode(lockPage: Page, manualPage: Page): Promise<number[]> {
  const tiles = await lockPage
    .getByTestId('lock-tile')
    .evaluateAll((els) => els.map((el) => ({ color: el.dataset.color!, number: Number(el.dataset.number) })))
  const rules = await manualPage
    .getByTestId('manual-rule')
    .evaluateAll((els) => els.map((el) => ({ color: el.dataset.color!, kind: el.dataset.kind! })))
  expect(tiles.length).toBeGreaterThan(0)
  expect(rules.length).toBe(3)
  return rules.map((rule) => {
    const nums = tiles.filter((t) => t.color === rule.color).map((t) => t.number)
    if (rule.kind === 'sum') return nums.reduce((a, b) => a + b, 0)
    if (rule.kind === 'count') return nums.length
    if (rule.kind === 'largest') return Math.max(...nums)
    return Math.min(...nums)
  })
}

async function typeCode(page: Page, code: number[]) {
  const boxes = page.getByTestId('code-digit')
  for (const [i, digit] of code.entries()) await boxes.nth(i).fill(String(digit))
  await page.getByRole('button', { name: 'Open the lock' }).click()
}

test('two players open the lock together by talking', async ({ browser }) => {
  const { host, code } = await hostCreatesGame(browser, 'Ms Lock')
  const ali = await newPage(browser)
  const bea = await newPage(browser)
  await joinAs(ali, code, 'Ali')
  await joinAs(bea, code, 'Bea')
  await ali.getByRole('button', { name: "I'm ready!" }).click()
  await bea.getByRole('button', { name: "I'm ready!" }).click()
  await host.getByRole('button', { name: 'Start the game' }).click()

  // Each kid only has half of the information.
  await expect(ali.getByRole('heading', { name: "The merchant's lock" })).toBeVisible()
  await expect(bea.getByRole('heading', { name: "The merchant's manual" })).toBeVisible()
  await expect(ali.getByTestId('manual-rule')).toHaveCount(0)
  await expect(bea.getByTestId('lock-tile')).toHaveCount(0)
  await expect(bea.getByTestId('code-digit')).toHaveCount(0)
  await expect(ali.getByText('Puzzle 1 of 1')).toBeVisible()
  await expect(host.getByText('Puzzle 1 of 1')).toBeVisible() // the host follows progress too

  const answer = await workOutCode(ali, bea)

  // A wrong try: both hear about it, and the manual reader sees what was tried.
  const wrong = answer.map((d) => (d === 9 ? 1 : d + 1))
  await typeCode(ali, wrong)
  for (const page of [ali, bea]) await expect(page.getByText('Not quite!')).toBeVisible()
  await expect(bea.getByTestId('tried-codes')).toContainText(wrong.join(' '))
  await expect(ali.getByTestId('code-digit').first()).toHaveValue('') // boxes cleared for the next try

  // The right code: everyone sees the escape.
  await typeCode(ali, answer)
  for (const page of [ali, bea]) {
    await expect(page.getByRole('heading', { name: 'You escaped!' })).toBeVisible()
    await expect(page.getByText(/Your team did it in \d+:\d\d\./)).toBeVisible()
  }
  await expect(host.getByRole('heading', { name: 'Game over' })).toBeVisible()
  await expect(host.getByRole('heading', { name: 'You escaped!' })).toBeVisible()
})

test('host removes a player from the lobby', async ({ browser }) => {
  const host = await newPage(browser)
  await host.goto('/host/login')
  await host.getByRole('button', { name: 'No account yet? Create one' }).click()
  await host.getByLabel('Your name (shown to you only)').fill('Mr Haddad')
  await host.getByLabel('Email').fill(`e2e-rm-${Date.now()}@example.com`)
  await host.getByLabel('Password').fill('password123')
  await host.getByRole('button', { name: 'Create account' }).click()
  await host.getByRole('button', { name: 'Create game' }).click()
  const code = (await host.getByTestId('room-code').textContent())!.trim()

  const kid = await newPage(browser)
  await joinAs(kid, code, 'Prankster')
  await expect(host.getByTestId('player-slot')).toHaveCount(1)

  host.once('dialog', (dialog) => void dialog.accept())
  await host.getByRole('button', { name: 'Remove Prankster' }).click()

  await expect(host.getByTestId('player-slot')).toHaveCount(0)
  await expect(kid.getByText('The host removed you from this game.')).toBeVisible()
})
