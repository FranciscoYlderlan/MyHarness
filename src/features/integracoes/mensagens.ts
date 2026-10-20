// Textos da TEL-02 e do CMP-03: nomes das integrações, status e a mensagem de cada ERR do
// catálogo (`Anexos/contrato/05-erros.md`). O núcleo manda o código e os valores `<x>`,
// já mascarados (RN-04); a tela escolhe a mensagem pelo código.
import type { ErroCatalogo, ErroIpc, StatusIntegracao, TipoIntegracao } from "@/lib/ipc";

export const ORDEM_DAS_INTEGRACOES: TipoIntegracao[] = ["claude_cli", "obsidian_cli", "github"];

export const NOME_DA_INTEGRACAO: Record<TipoIntegracao, string> = {
  claude_cli: "Claude CLI",
  obsidian_cli: "Obsidian CLI",
  github: "GitHub",
};

export const NOME_DO_STATUS: Record<StatusIntegracao, string> = {
  conectado: "conectado",
  desconectado: "desconectado",
  sem_login: "sem login",
  nao_instalado: "não instalado",
};

/** Link de instalação do Claude CLI mostrado com o ERR-01 [a confirmar]. */
export const LINK_DE_INSTALACAO_DO_CLAUDE_CLI = "https://docs.claude.com/en/docs/claude-code/setup";

/** Abre o Obsidian pelo protocolo dele (ação "Abrir Obsidian" do ERR-03). */
export const LINK_ABRIR_OBSIDIAN = "obsidian://open";

/** Tratamento do catálogo que permite tentar de novo a mesma operação. */
const REPETIVEIS = new Set<ErroCatalogo["codigo"]>([
  "LOGIN_CLAUDE_CLI_FALHOU",
  "INTEGRACAO_FALHA_INESPERADA",
  "FALHA_INTERNA",
]);

export function podeRepetir(erro: ErroCatalogo): boolean {
  return REPETIVEIS.has(erro.codigo);
}

/** Estado final da integração depois do erro (coluna "Estado final" do catálogo). */
export function statusDepoisDoErro(erro: ErroCatalogo): StatusIntegracao | null {
  switch (erro.codigo) {
    case "CLAUDE_CLI_NAO_INSTALADO":
      return "nao_instalado";
    case "LOGIN_CLAUDE_CLI_FALHOU":
      return "sem_login";
    case "GITHUB_OAUTH_NAO_CONCLUIDO":
    case "OBSIDIAN_FECHADO":
    case "INTEGRACAO_FALHA_INESPERADA":
      return "desconectado";
    default:
      return null;
  }
}

/** `<motivo>` do ERR-02, nas palavras do catálogo (M-47); o núcleo manda o código. */
const MOTIVO_DO_OAUTH: Partial<Record<string, string>> = {
  cancelado: "cancelada",
  access_denied: "recusada no GitHub",
  expired_token: "código expirado",
};

/**
 * Mensagem ao usuário de um erro do catálogo, com os valores do momento. Só texto do
 * catálogo: sem o valor `<x>` que a mensagem exige, a tela mostra o id e o código do erro,
 * em vez de inventar um texto.
 */
export function mensagemDoErro(erro: ErroCatalogo, tipo: TipoIntegracao): string {
  const p = erro.parametros;
  const nome = NOME_DA_INTEGRACAO[tipo];
  const semValor = `${erro.id} ${erro.codigo}`;
  switch (erro.codigo) {
    case "CLAUDE_CLI_NAO_INSTALADO":
      return "Claude CLI não encontrado";
    case "GITHUB_OAUTH_NAO_CONCLUIDO": {
      // Motivo fora do catálogo não aparece cru (M-47): a tela mostra o id e o código.
      const motivo = p.motivo ? MOTIVO_DO_OAUTH[p.motivo] : undefined;
      return motivo ? `Conexão com o GitHub não concluída: ${motivo}.` : semValor;
    }
    case "OBSIDIAN_FECHADO":
      return p.mensagem_da_cli || semValor;
    case "LOGIN_CLAUDE_CLI_FALHOU":
      return p.motivo || semValor;
    case "CLAUDE_CLI_DESCONECTADO":
      return "Conecte o Claude CLI para enviar";
    case "GITHUB_DESCONECTADO":
      return "Conecte o GitHub para aprovar este plano";
    case "OBSIDIAN_DESCONECTADO":
      return "Conecte o Obsidian para registrar";
    case "INTEGRACAO_FALHA_INESPERADA":
      return p.motivo ? `Falha inesperada com ${nome}: ${p.motivo}. Tente de novo.` : semValor;
    case "FALHA_INTERNA":
      return p.motivo ? `Algo deu errado no MyHarness: ${p.motivo}. Tente de novo.` : semValor;
    default:
      return semValor;
  }
}

/**
 * Converte o erro de um comando (ou a falha de chamar o núcleo) num item do catálogo. Falha
 * de chamar o núcleo vira o ERR-55 `FALHA_INTERNA` com o motivo.
 */
export function erroDoComando(erro: ErroIpc | unknown): ErroCatalogo {
  if (typeof erro === "object" && erro !== null && "tipo" in erro) {
    const e = erro as ErroIpc;
    if (e.tipo === "catalogo") return e.erro;
  }
  return {
    codigo: "FALHA_INTERNA",
    id: "ERR-55",
    parametros: { motivo: "o núcleo não respondeu" },
  };
}
