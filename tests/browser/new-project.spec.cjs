'use strict';

const { test, expect } = require('@playwright/test');

test('browser preview creates a project and enters the editor', async ({ page }) => {
  const pageErrors = [];
  const failedLocalResources = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  page.on('response', (response) => {
    const url = new URL(response.url());
    if (url.origin === 'http://127.0.0.1:1420' && response.status() >= 400) {
      failedLocalResources.push(`${response.status()} ${url.pathname}`);
    }
  });

  await page.goto('/');
  await page.evaluate(() => localStorage.clear());
  await page.reload();

  await expect(page.locator('#start-screen')).toBeVisible();
  await expect(page.locator('#start-brand-title')).toHaveText('Nemo');
  await expect(page.locator('#status-text')).toContainText('Nemo v');
  await expect.poll(() => page.evaluate(() => typeof window.SMProject?.newProject)).toBe('function');

  await page.locator('#start-new').click();
  await expect(page.locator('#start-newpanel')).toBeVisible();
  await page.locator('#np-name').fill('Hosted browser check');
  await page.locator('#np-preset').selectOption('1280x720');
  await page.locator('#np-fps').selectOption('24');
  await page.locator('#np-create').click();

  await expect(page.locator('#start-screen')).toHaveClass(/\bhid\b/);
  await expect(page.locator('#drawing-canvas')).toBeVisible();
  await expect(page.locator('#project-tabs-list .project-tab')).toHaveCount(1);
  await expect(page.locator('#project-tabs-list .project-tab')).toContainText('Hosted browser check');
  await expect(page.locator('#info-frame')).toHaveText('1');
  const canvasSize = await page.locator('#drawing-canvas').evaluate((canvas) => ({
    width: canvas.width,
    height: canvas.height,
  }));
  expect(canvasSize.width).toBeGreaterThan(0);
  expect(canvasSize.height).toBeGreaterThan(0);

  expect(failedLocalResources, `local resource failures: ${failedLocalResources.join(', ')}`).toEqual([]);
  expect(pageErrors, `uncaught page errors: ${pageErrors.join(' | ')}`).toEqual([]);
});
