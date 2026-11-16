// Renders docs/assets/hero.html to docs/assets/hero.png (the README header) with Playwright.
// Run from the repository root: node scripts/hero.mjs
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const require = createRequire(import.meta.url)
const { chromium } = require('playwright')
const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'docs', 'assets')

const browser = await chromium.launch()
const page = await browser.newPage({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 2 })
await page.goto('file://' + path.join(dir, 'hero.html'))
await page.evaluate(() => document.fonts.ready)
await page.locator('.card').screenshot({ path: path.join(dir, 'hero.png'), omitBackground: false })
await browser.close()
console.log('wrote docs/assets/hero.png')
