import { createFileRoute } from "@tanstack/react-router";
import { ImageOff } from "lucide-react";

export const Route = createFileRoute("/projects/$projectId/resources")({ component: ResourcesPage });

function ResourcesPage() {
  return (
    <div className="grid h-full place-items-center bg-[var(--surface-inset)] p-8">
      <div className="max-w-[420px] text-center">
        <ImageOff size={28} className="mx-auto text-[var(--text-muted)]" />
        <h1 className="mb-2 mt-4 text-base font-semibold">没有可处理的图片文字区域</h1>
        <p className="m-0 text-sm leading-6 text-[var(--text-secondary)]">
          当前项目尚未完成图片文字识别。原始图片仍保留在项目中。
        </p>
      </div>
    </div>
  );
}
