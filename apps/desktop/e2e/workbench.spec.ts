import { expect, test } from "@playwright/test";

test("opens the project workbench with a stable translation shell", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "继续工作" })).toBeVisible();

  await page.getByRole("link", { name: "继续翻译" }).click();
  await expect(page).toHaveURL(/\/projects\/preview\/content$/);
  await expect(page.getByRole("heading", { name: "原文" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "译文" })).toBeVisible();

  await page.getByRole("link", { name: "单元" }).click();
  await expect(page).toHaveURL(/\/projects\/preview\/units$/);
  await expect(page.getByText("第 1 单元")).toBeVisible();

  await page.getByRole("link", { name: "资源" }).click();
  await expect(page).toHaveURL(/\/projects\/preview\/resources$/);
  await expect(page.getByText("当前项目尚未生成图片文字区域")).toBeVisible();
});
