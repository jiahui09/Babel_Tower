import { expect, test } from "@playwright/test";

test("opens the project workbench with a stable translation shell", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "继续工作" })).toBeVisible();

  await page.getByRole("link", { name: "继续翻译" }).click();
  await expect(page).toHaveURL(/\/projects\/fixture-project\/content$/);
  await expect(page.getByText("The tower keeps the original safe.")).toBeVisible();
  await expect(page.getByText("译文")).toBeVisible();

  await page.getByRole("link", { name: "单元" }).click();
  await expect(page).toHaveURL(/\/projects\/fixture-project\/units$/);
  await expect(page.getByText("单元")).toBeVisible();

  await page.getByRole("link", { name: "资源" }).click();
  await expect(page).toHaveURL(/\/projects\/fixture-project\/resources$/);
  await expect(page.getByText("没有可处理的图片文字区域")).toBeVisible();
});
