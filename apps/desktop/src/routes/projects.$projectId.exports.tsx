import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowLeft, FileOutput } from "lucide-react";

import { buttonVariants } from "../components/ui/button";

export const Route = createFileRoute("/projects/$projectId/exports")({ component: ExportsPage });

function ExportsPage() {
  const { projectId } = Route.useParams();
  return (
    <div className="h-full overflow-auto p-8">
      <div className="mx-auto max-w-[860px]">
        <Link
          to="/projects/$projectId/content"
          params={{ projectId }}
          className="flex items-center gap-2 text-sm text-[var(--text-secondary)]"
        >
          <ArrowLeft size={15} />
          返回翻译
        </Link>
        <div className="mt-8 flex items-center">
          <div>
            <h1 className="m-0 text-xl font-semibold">导出记录</h1>
            <p className="mb-0 mt-2 text-sm text-[var(--text-secondary)]">创建新文件，原件不会被修改。</p>
          </div>
          <button className={`${buttonVariants({ variant: "primary" })} ml-auto`}>
            <FileOutput size={16} />
            新建导出
          </button>
        </div>
        <div className="mt-8 border-y border-[var(--border)] py-8 text-center text-sm text-[var(--text-muted)]">
          尚无导出记录
        </div>
      </div>
    </div>
  );
}
