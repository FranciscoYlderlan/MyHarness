// Cenários do modo simulado (`pnpm dev:simulado`, `?cenario=<nome>` na URL): o estado em
// que o núcleo simulado abre a TEL-02. A lista mora só aqui (nome → estado); os testes do
// vitest abrem a tela em cada um.
import type { ErroIpc, Integracao, TipoIntegracao } from "@/bindings";
import {
  erroDoCatalogo,
  falhaInesperada,
  falhaInterna,
} from "@/lib/ipc/nucleo-simulado-integracoes";

type EstadoGuardado = Partial<Omit<Integracao, "tipo">>;

export type Cenario = {
  /** O que a TEL-02 mostra ao abrir, em uma frase. */
  descricao: string;
  /** O que muda no estado inicial padrão, por integração. */
  estado?: Partial<Record<TipoIntegracao, EstadoGuardado>>;
  /**
   * ERR-55: a integração que o núcleo não ligou. Abre `desconectado`; Conectar (e Testar,
   * Desconectar) falha sempre com este erro, como no núcleo real.
   */
  naoLigada?: { tipo: TipoIntegracao; erro: ErroIpc };
};

/** Estado inicial padrão: Claude CLI pronto, Obsidian CLI e GitHub a conectar. */
export const ESTADO_INICIAL_SIMULADO = {
  claude_cli: { status: "conectado", versao: "2.1.0 (Claude Code) [simulado]" },
  obsidian_cli: { status: "desconectado" },
  github: { status: "desconectado" },
} as const satisfies Record<TipoIntegracao, EstadoGuardado>;

const PREFIXO: Record<TipoIntegracao, string> = {
  claude_cli: "claude",
  obsidian_cli: "obsidian",
  github: "github",
};

const NOME: Record<TipoIntegracao, string> = {
  claude_cli: "Claude CLI",
  obsidian_cli: "Obsidian CLI",
  github: "GitHub",
};

const TIPOS = Object.keys(PREFIXO) as TipoIntegracao[];

/** `sem login`, `não instalado`, ERR-53 e ERR-55 valem para as três integrações. */
function cenariosDeCadaIntegracao(): Record<string, Cenario> {
  const cenarios: Record<string, Cenario> = {};
  for (const tipo of TIPOS) {
    const p = PREFIXO[tipo];
    const nome = NOME[tipo];
    cenarios[`${p}-sem-login`] = {
      descricao: `${nome} sem login`,
      estado: { [tipo]: { status: "sem_login", conta: null, versao: null } },
    };
    cenarios[`${p}-nao-instalado`] = {
      descricao: `${nome} não instalado, sem erro no estado`,
      estado: { [tipo]: { status: "nao_instalado", conta: null, versao: null } },
    };
    cenarios[`${p}-err-53`] = {
      descricao: `${nome} desconectado com o ERR-53 (falha inesperada)`,
      estado: {
        [tipo]: {
          status: "desconectado",
          conta: null,
          versao: null,
          erro: falhaInesperada(tipo, "tempo esgotado [simulado]").erro,
        },
      },
    };
    cenarios[`${p}-err-55`] = {
      descricao: `${nome} não ligada no núcleo: abre desconectado; Conectar dá o ERR-55`,
      naoLigada: { tipo, erro: falhaInterna(`o núcleo não ligou ${nome} [simulado]`) },
    };
  }
  return cenarios;
}

/** Os cenários do `dev:simulado`, por nome (kebab-case). */
export const CENARIOS: Record<string, Cenario> = {
  padrao: { descricao: "Claude CLI conectado; Obsidian CLI e GitHub desconectados" },
  "todas-conectadas": {
    descricao: "as três integrações conectadas",
    estado: {
      obsidian_cli: { status: "conectado", notas_indexadas: 128 },
      github: { status: "conectado", conta: "conta-simulada" },
    },
  },
  "claude-err-01": {
    descricao: "Claude CLI não instalado, com o ERR-01 no estado",
    estado: {
      claude_cli: {
        status: "nao_instalado",
        versao: null,
        erro: erroDoCatalogo("CLAUDE_CLI_NAO_INSTALADO", "ERR-01").erro,
      },
    },
  },
  "github-err-02": {
    descricao: "GitHub desconectado com o ERR-02 (código do device flow expirado)",
    estado: {
      github: {
        status: "desconectado",
        erro: erroDoCatalogo("GITHUB_OAUTH_NAO_CONCLUIDO", "ERR-02", { motivo: "expired_token" })
          .erro,
      },
    },
  },
  "obsidian-err-03": {
    descricao: "Obsidian CLI desconectado com o ERR-03 (Obsidian fechado)",
    estado: {
      obsidian_cli: {
        status: "desconectado",
        erro: erroDoCatalogo("OBSIDIAN_FECHADO", "ERR-03", {
          mensagem_da_cli: "The CLI is unable to find Obsidian [simulado]",
        }).erro,
      },
    },
  },
  "claude-err-04": {
    descricao: "Claude CLI sem login com o ERR-04 (login falhou)",
    estado: {
      claude_cli: {
        status: "sem_login",
        versao: null,
        erro: erroDoCatalogo("LOGIN_CLAUDE_CLI_FALHOU", "ERR-04", {
          motivo: "o /login foi fechado antes de concluir [simulado]",
        }).erro,
      },
    },
  },
  ...cenariosDeCadaIntegracao(),
};

export const CENARIO_PADRAO = "padrao";

/**
 * O cenário pedido na URL (`?cenario=<nome>`). Sem o parâmetro, o padrão. Nome
 * desconhecido: o padrão, com a lista de nomes no console do navegador.
 */
export function cenarioDaUrl(busca: string): string {
  const pedido = new URLSearchParams(busca).get("cenario");
  if (!pedido) return CENARIO_PADRAO;
  if (Object.hasOwn(CENARIOS, pedido)) return pedido;
  console.warn(
    `[MyHarness] cenário "${pedido}" desconhecido; usando "${CENARIO_PADRAO}". ` +
      `Cenários: ${Object.keys(CENARIOS).join(", ")}`,
  );
  return CENARIO_PADRAO;
}
