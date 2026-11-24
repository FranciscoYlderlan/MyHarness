// CMP-03 Indicador de integrações (TEL-01, canto superior direito): um sinal por
// integração; o clique abre a TEL-02 (NAV-03).
import { useIntegracoes } from "./contexto";
import { situacao, type Situacao } from "./estado";
import { NOME_DA_INTEGRACAO, NOME_DO_STATUS, ORDEM_DAS_INTEGRACOES } from "./mensagens";

const COR: Record<Situacao, string> = {
  carregando: "bg-muted-foreground animate-pulse",
  conectado: "bg-emerald-500",
  erro: "bg-destructive",
};

export function IndicadorIntegracoes({ aoAbrir }: { aoAbrir: () => void }) {
  const { estado } = useIntegracoes();
  const itens = ORDEM_DAS_INTEGRACOES.map((tipo) => {
    const e = estado[tipo];
    const sit = situacao(e);
    const texto =
      sit === "carregando"
        ? "checando"
        : e.integracao
          ? NOME_DO_STATUS[e.integracao.status]
          : "com erro";
    return { tipo, sit, rotulo: `${NOME_DA_INTEGRACAO[tipo]}: ${texto}` };
  });

  const geral: Situacao = itens.some((i) => i.sit === "erro")
    ? "erro"
    : itens.some((i) => i.sit === "carregando")
      ? "carregando"
      : "conectado";
  const resumo =
    geral === "erro"
      ? "integração desconectada"
      : geral === "carregando"
        ? "checando"
        : "todas conectadas";

  return (
    <button
      type="button"
      onClick={aoAbrir}
      data-situacao={geral}
      aria-label={`Integrações: ${resumo}. ${itens.map((i) => i.rotulo).join("; ")}`}
      className={`flex items-center gap-3 rounded-lg border px-3 py-1.5 text-xs hover:bg-muted ${
        geral === "erro" ? "border-destructive" : ""
      }`}
    >
      {itens.map((i) => (
        <span key={i.tipo} data-integracao={i.tipo} data-situacao={i.sit} title={i.rotulo}>
          <span className="flex items-center gap-1.5">
            <span aria-hidden className={`size-2 rounded-full ${COR[i.sit]}`} />
            {NOME_DA_INTEGRACAO[i.tipo]}
          </span>
        </span>
      ))}
    </button>
  );
}
