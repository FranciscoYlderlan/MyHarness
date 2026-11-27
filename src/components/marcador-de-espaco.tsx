// Espaço reservado de um componente lógico (CMP) na tela: só o ID e o nome, até a tarefa
// dona da funcionalidade trocar o marcador pelo componente de verdade.
import { cn } from "@/lib/utils";

export function MarcadorDeEspaco({
  cmp,
  nome,
  className,
}: {
  /** ID do componente no contrato, ex.: "CMP-01". */
  cmp: string;
  /** Nome do componente no contrato. */
  nome: string;
  className?: string;
}) {
  return (
    <section
      aria-label={`${cmp} ${nome}`}
      data-cmp={cmp}
      className={cn(
        "flex h-full min-h-16 flex-col items-center justify-center gap-0.5 rounded-lg border border-dashed p-3 text-center text-muted-foreground",
        className,
      )}
    >
      <span className="font-mono text-xs">{cmp}</span>
      <span className="text-sm">{nome}</span>
    </section>
  );
}
