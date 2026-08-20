import { createFileRoute, Link } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";

import { buttonVariants } from "../components/ui/button";

export const Route = createFileRoute("/recovery/$projectId")({ component: RecoveryPage });

function RecoveryPage() {
  const { projectId } = Route.useParams();
  const { t } = useTranslation(["recovery", "common"]);
  return (
    <div className="grid h-full place-items-center bg-[var(--surface)]">
      <div className="w-[560px] border border-[var(--border)] bg-[var(--surface-raised)] p-6">
        <h1 className="m-0 text-lg font-semibold">{t("title")}</h1>
        <p className="mt-3 text-sm leading-6 text-[var(--text-secondary)]">{t("description")}</p>
        <div className="mt-6 flex justify-end gap-2">
          <Link to="/" className={buttonVariants({ variant: "secondary" })}>
            {t("backToLibrary")}
          </Link>
          <Link
            to="/projects/$projectId/content"
            params={{ projectId }}
            search={{ unitId: undefined }}
            className={buttonVariants({ variant: "primary" })}
          >
            {t("resume")}
          </Link>
        </div>
      </div>
    </div>
  );
}
