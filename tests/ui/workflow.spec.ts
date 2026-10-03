import AxeBuilder from '@axe-core/playwright';
import { test, expect } from '@playwright/test';
test('sample preview, assignment edits and operation switches', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/');
  await expect(
    page.getByRole('heading', { name: '動画を整理', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'サンプルで試す' }).click();
  await expect(
    page.getByRole('heading', { name: '星をつなぐ旅' }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'プレビュー', exact: true }).click();
  await expect(page.getByText('4件の動画を処理できます')).toBeVisible();
  await expect(
    page
      .getByRole('cell')
      .filter({ hasText: '→ 星をつなぐ旅 - 第01話 - 旅のはじまり.mkv' }),
  ).toBeVisible();
  await page
    .getByRole('button', { name: '星をつなぐ旅 第01話.mkvの詳細を編集' })
    .click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.getByLabel('サブタイトル', { exact: true }).fill('新しい題名');
  await page.getByRole('button', { name: '適用', exact: true }).click();
  await expect(
    page.getByRole('button', { name: /サンプル実行/ }),
  ).toBeDisabled();
  await page.getByRole('button', { name: 'プレビュー', exact: true }).click();
  await expect(
    page
      .getByRole('cell')
      .filter({ hasText: '→ 星をつなぐ旅 - 第01話 - 新しい題名.mkv' }),
  ).toBeVisible();
  await page.getByRole('switch', { name: 'タグを付与', exact: true }).click();
  await page.getByRole('switch', { name: 'リネーム', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'プレビュー', exact: true }),
  ).toBeDisabled();
  expect(errors).toEqual([]);
});
test('automatic episode matching restores unassigned rows without changing manual choices', async ({
  page,
}) => {
  await page.goto('/');
  const match = page.getByRole('button', { name: '自動マッチ', exact: true });
  await expect(match).toBeDisabled();
  await page.getByRole('button', { name: 'サンプルで試す' }).click();
  const first = page.getByRole('combobox', {
    name: '星をつなぐ旅 第01話.mkvのエピソード',
    exact: true,
  });
  const second = page.getByRole('combobox', {
    name: '星をつなぐ旅 第02話.mkvのエピソード',
    exact: true,
  });
  await first.selectOption('');
  await second.selectOption('3');
  await match.click();
  await expect(first).toHaveValue('1');
  await expect(second).toHaveValue('3');
  await expect(page.getByRole('status')).toContainText('1件をマッチしました');
});

test('light and dark layouts remain inside the viewport', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'サンプルで試す' }).click();
  await page.getByRole('button', { name: 'プレビュー', exact: true }).click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: 'test-results/animeta-light.png',
    fullPage: true,
  });
  await page.getByRole('button', { name: '設定', exact: true }).click();
  await page.getByLabel('テーマ', { exact: true }).selectOption('dark');
  await page.getByRole('button', { name: '動画を整理', exact: true }).click();
  await expect(page.locator('html')).toHaveClass(/dark/);
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: 'test-results/animeta-dark.png',
    fullPage: true,
  });
  await page.setViewportSize({ width: 960, height: 640 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
});
test('keyboard dialog interaction and empty history', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '作品を検索', exact: true }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.getByLabel('作品名', { exact: true }).fill('星');
  await page.getByRole('button', { name: '検索', exact: true }).click();
  await page.getByRole('button', { name: /星をつなぐ旅/ }).click();
  await expect(page.getByRole('dialog')).not.toBeVisible();
  await page.getByRole('button', { name: '処理履歴', exact: true }).click();
  await expect(
    page.getByRole('heading', { name: '処理履歴はまだありません' }),
  ).toBeVisible();
});

test('sample screens meet automated WCAG checks in both themes', async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.goto('/');
  await page.getByRole('button', { name: 'サンプルで試す' }).click();
  for (const theme of ['light', 'dark']) {
    await page.getByRole('button', { name: '設定', exact: true }).click();
    await page.getByLabel('テーマ', { exact: true }).selectOption(theme);
    await page.getByRole('button', { name: '動画を整理', exact: true }).click();
    const results = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa'])
      .analyze();
    expect(results.violations).toEqual([]);
  }
});

test('help is disclosed on demand and returns keyboard focus', async ({
  page,
}) => {
  await page.goto('/');
  const help = page.getByRole('button', {
    name: '動画を整理のヘルプ',
    exact: true,
  });
  await expect(
    page.getByText('元動画を保持し、出力先に新しい動画を作成します。', {
      exact: false,
    }),
  ).not.toBeVisible();
  await help.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(
    page.getByRole('heading', { name: 'ファイルの保存' }),
  ).toBeVisible();
  expect(
    (await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa']).analyze())
      .violations,
  ).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(help).toBeFocused();
  await page.getByRole('button', { name: '命名のヘルプ', exact: true }).click();
  await expect(
    page.getByRole('dialog').getByText('省略する区間'),
  ).toBeVisible();
  await page.getByRole('button', { name: '閉じる', exact: true }).click();
  await page.getByRole('button', { name: '設定', exact: true }).click();
  await page
    .getByRole('button', { name: '動画処理ツールのヘルプ', exact: true })
    .click();
  await expect(
    page.getByRole('button', { name: 'FFmpeg公式ダウンロード' }),
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(
    page.getByRole('button', { name: 'FFmpeg公式ダウンロード' }),
  ).not.toBeVisible();
});

test('removal operations work independently and require a fresh preview', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'サンプルで試す' }).click();
  const preview = page.getByRole('button', { name: 'プレビュー', exact: true });
  const execute = page.getByRole('button', { name: /サンプル実行/ });
  await page.getByRole('switch', { name: 'リネーム', exact: true }).click();
  await page.getByRole('switch', { name: 'タグを付与', exact: true }).click();
  await expect(preview).toBeDisabled();
  const subtitles = page.getByRole('switch', {
    name: '字幕を削除',
    exact: true,
  });
  const chapters = page.getByRole('switch', {
    name: 'チャプターを削除',
    exact: true,
  });
  await expect(subtitles).not.toBeChecked();
  await expect(chapters).not.toBeChecked();
  await subtitles.click();
  await expect(preview).toBeEnabled();
  await preview.click();
  await expect(execute).toBeEnabled();
  await chapters.click();
  await expect(execute).toBeDisabled();
  await preview.click();
  await expect(execute).toBeEnabled();
  await subtitles.click();
  await expect(preview).toBeEnabled();
  await chapters.click();
  await expect(preview).toBeDisabled();
});

test('replacement mode uses original folders and changing modes invalidates preview', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'サンプルで試す' }).click();
  await page.getByRole('button', { name: '設定', exact: true }).click();
  const mode = page.getByLabel('リネーム方法', { exact: true });
  await expect(mode).toHaveValue('copy');
  await mode.selectOption('replace');
  await page.getByRole('button', { name: '設定を保存' }).click();
  await page.getByRole('button', { name: '動画を整理', exact: true }).click();
  await expect(
    page.getByLabel('出力先フォルダ', { exact: true }),
  ).not.toBeVisible();
  await expect(
    page.getByText('元動画を置き換え', { exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'プレビュー', exact: true }).click();
  await page
    .getByRole('button', { name: '星をつなぐ旅 第01話.mkvの詳細を編集' })
    .click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: '設定', exact: true }).click();
  await mode.selectOption('copy');
  await page.getByRole('button', { name: '動画を整理', exact: true }).click();
  await expect(
    page.getByLabel('出力先フォルダ', { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole('button', { name: /サンプル実行/ }),
  ).toBeDisabled();
});
