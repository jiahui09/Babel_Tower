import { createFileRoute, Link } from "@tanstack/react-router";
import { AlertTriangle, ArrowLeft } from "lucide-react";

export const Route = createFileRoute("/projects/$projectId/validate")({ component: ValidationPage });

function ValidationPage() {
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
        <h1 className="mt-8 text-xl font-semibold">2 个问题需要处理</h1>
        <div className="mt-6 border-y border-[var(--border)] bg-[var(--surface-raised)]">
          <div className="flex gap-3 border-b border-[var(--border)] p-4">
            <AlertTriangle size={18} className="text-[var(--danger)]" />
            <div>
              <strong className="text-sm">第 31 单元缺少必需译文</strong>
              <p className="mb-0 mt-1 text-xs text-[var(--text-secondary)]">
                此内容为空，因此暂时无法安全导出。
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
