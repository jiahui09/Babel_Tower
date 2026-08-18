import { createFileRoute, Link } from "@tanstack/react-router";

import { buttonVariants } from "../components/ui/button";

export const Route = createFileRoute("/recovery/$projectId")({ component: RecoveryPage });

function RecoveryPage() {
  const { projectId } = Route.useParams();
  return (
    <div className="grid h-full place-items-center bg-[var(--surface)]">
      <div className="w-[560px] border border-[var(--border)] bg-[var(--surface-raised)] p-6">
        <h1 className="m-0 text-lg font-semibold">发现未确认的编辑内容</h1>
        <p className="mt-3 text-sm leading-6 text-[var(--text-secondary)]">
          上次关闭前仍有本地草稿。已保存的译文没有改变。
        </p>
        <div className="mt-6 flex justify-end gap-2">
          <Link to="/" className={buttonVariants({ variant: "secondary" })}>
            返回项目库
          </Link>
          <Link
            to="/projects/$projectId/content"
            params={{ projectId }}
            className={buttonVariants({ variant: "primary" })}
          >
            恢复并继续
          </Link>
        </div>
      </div>
    </div>
  );
}
