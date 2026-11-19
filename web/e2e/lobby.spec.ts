import { type Browser, type Page, expect, test } from '@playwright/test'

// M1 "done" scenario: two browser windows join the same room and see each other in the lobby;
// the host starts the game. Each actor gets its own browser context (separate cookies/storage).

async function newPage(browser: Browser): Promise<Page> {
  const context = await browser.newContext()
  return context.newPage()
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
    await expect(page.getByRole('heading', { name: 'The adventure begins!' })).toBeVisible()
    await expect(page.getByRole('timer')).toHaveText(/^(20:00|19:5\d)$/)
  }
  await expect(host.getByRole('heading', { name: 'Game in progress' })).toBeVisible()

  // Reloading a player's tab keeps them in the game (token survives in sessionStorage).
  await ali.reload()
  await expect(ali.getByRole('heading', { name: 'The adventure begins!' })).toBeVisible()
})
