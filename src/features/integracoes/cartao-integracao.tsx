// CMP-11 Cartão de integração (TEL-02): status, conta ou versão, instruções de conexão,
// o erro do catálogo com a ação dele e os botões Conectar, Desconectar, Testar e Repetir.
import { Button } from "@/components/ui/button";
import type { TipoIntegracao } from "@/lib/ipc";
import { useIntegracoes } from "./contexto";
import { situacao, type Operacao } from "./estado";
import {
  LINK_ABRIR_OBSIDIAN,
  LINK_DE_INSTALACAO_DO_CLAUDE_CLI,
  mensagemDoErro,
  NOME_DA_INTEGRACAO,
  NOME_DO_STATUS,
  podeRepetir,
} from "./mensagens";

const EM_ANDAMENTO: Record<Operacao, (tipo: TipoIntegracao) => string> = {
  checar: () => "checando",
  conectar: (tipo) =>
    tipo === "obsidian_cli" ? "testando o servidor MCP da Obsidian CLI" : "login em andamento",
  desconectar: () => "desconectando",
};

const COR: Record<ReturnType<typeof situacao>, string> = {
  carregando: "bg-muted-foreground",
  conectado: "bg-emerald-500",
  erro: "bg-destructive",
};

export function CartaoIntegracao({ tipo }: { tipo: TipoIntegracao }) {
  const { estado, checar, conectar, desconectar, repetir } = useIntegracoes();
  const e = estado[tipo];
  const nome = NOME_DA_INTEGRACAO[tipo];
  const sit = situacao(e);
  const status = e.integracao?.status ?? null;
  const ocupado = e.emAndamento !== null;
  const erro = e.erro?.erro ?? null;
  const naoInstalado = status === "nao_instalado" || erro?.codigo === "CLAUDE_CLI_NAO_INSTALADO";
  const repetivel = erro !== null && podeRepetir(erro);
  const repetirEhConectar = repetivel && e.erro?.operacao === "conectar";

  const acoes: { rotulo: string; aoClicar: () => void }[] = [];
  if (!ocupado) {
    if (status === "conectado") {
      acoes.push({ rotulo: "Testar", aoClicar: () => checar(tipo) });
      acoes.push({ rotulo: "Desconectar", aoClicar: () => desconectar(tipo) });
    } else if (naoInstalado) {
      acoes.push({ rotulo: "Testar", aoClicar: () => checar(tipo) });
    } else if (!repetirEhConectar) {
      acoes.push({ rotulo: "Conectar", aoClicar: () => conectar(tipo) });
    }
    if (repetivel) acoes.push({ rotulo: "Repetir", aoClicar: () => repetir(tipo) });
  }

  const tituloId = `integracao-${tipo}-titulo`;

  return (
    <article
      aria-labelledby={tituloId}
      data-situacao={sit}
      className={`rounded-lg border p-4 ${sit === "erro" ? "border-destructive" : ""}`}
    >
      <header className="flex items-center gap-2">
        <span aria-hidden className={`size-2.5 rounded-full ${COR[sit]}`} />
        <h3 id={tituloId} className="font-medium">
          {nome}
        </h3>
        <span className="ml-auto text-sm text-muted-foreground">
          {ocupado
            ? EM_ANDAMENTO[e.emAndamento!](tipo)
            : status
              ? NOME_DO_STATUS[status]
              : "checando"}
        </span>
      </header>

      {!ocupado && status === "conectado" && erro === null && (
        <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 text-sm">
          {e.integracao?.conta && (
            <>
              <dt className="text-muted-foreground">Conta</dt>
              <dd>{e.integracao.conta}</dd>
            </>
          )}
          {e.integracao?.versao && (
            <>
              <dt className="text-muted-foreground">Versão</dt>
              <dd>{e.integracao.versao}</dd>
            </>
          )}
          {e.integracao?.notas_indexadas != null && (
            <>
              <dt className="text-muted-foreground">Notas indexadas</dt>
              <dd>{e.integracao.notas_indexadas}</dd>
            </>
          )}
        </dl>
      )}

      {e.instrucoes.length > 0 && (
        <ul aria-label={`Instruções de conexão do ${nome}`} className="mt-2 space-y-1 text-sm">
          {e.instrucoes.map((instrucao, i) =>
            instrucao.tipo === "codigo_do_dispositivo" ? (
              <li key={i}>
                Digite o código{" "}
                <code className="rounded bg-muted px-1 font-mono text-base font-semibold">
                  {instrucao.codigo}
                </code>{" "}
                em{" "}
                <a
                  href={instrucao.endereco}
                  target="_blank"
                  rel="noreferrer"
                  className="text-primary underline"
                >
                  {instrucao.endereco.replace(/^https?:\/\//, "")}
                </a>
              </li>
            ) : (
              <li key={i}>{instrucao.texto}</li>
            ),
          )}
        </ul>
      )}

      {!ocupado && (erro !== null || naoInstalado) && (
        <div role="alert" className="mt-2 text-sm text-destructive">
          <p>
            {erro !== null
              ? mensagemDoErro(erro, tipo)
              : mensagemDoErro(
                  { codigo: "CLAUDE_CLI_NAO_INSTALADO", id: "ERR-01", parametros: {} },
                  tipo,
                )}
          </p>
          {naoInstalado && (
            <a
              href={LINK_DE_INSTALACAO_DO_CLAUDE_CLI}
              target="_blank"
              rel="noreferrer"
              className="text-primary underline"
            >
              Instalar o Claude CLI
            </a>
          )}
          {erro?.codigo === "OBSIDIAN_FECHADO" && (
            <a href={LINK_ABRIR_OBSIDIAN} className="text-primary underline">
              Abrir Obsidian
            </a>
          )}
        </div>
      )}

      {acoes.length > 0 && (
        <div className="mt-3 flex gap-2">
          {acoes.map((a) => (
            <Button
              key={a.rotulo}
              size="sm"
              variant={a.rotulo === "Conectar" ? "default" : "outline"}
              onClick={a.aoClicar}
            >
              {a.rotulo}
            </Button>
          ))}
        </div>
      )}
    </article>
  );
}
