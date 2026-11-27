// Integrações (TEL-02, CMP-03, CMP-11) no modo simulado: os cenários em que a TEL-02 abre
// e o roteiro de uma sessão de desenvolvimento, sobre o mesmo núcleo simulado dos testes do
// vitest (`@/lib/ipc/nucleo-simulado-integracoes`).
import type { ErroIpc, InstrucaoConexao, Integracao, TipoIntegracao } from "@/bindings";
import { definirFuncionalidade } from "@/dev/funcionalidade-simulada";
import {
  criarNucleoSimuladoDeIntegracoes,
  erroDoCatalogo,
  falhaInesperada,
  falhaInterna,
  type NucleoSimuladoDeIntegracoes,
} from "@/lib/ipc/nucleo-simulado-integracoes";

declare module "@/dev/funcionalidade-simulada" {
  interface ControlesSimulados {
    /** O núcleo simulado das integrações, para mudar o estado à mão. */
    integracoes: NucleoSimuladoDeIntegracoes;
  }
}

type EstadoGuardado = Partial<Omit<Integracao, "tipo">>;

type CenarioDeIntegracoes = {
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

/** Instruções que cada `conectar` manda pelo canal, como o núcleo real. */
export const INSTRUCOES_SIMULADAS: Record<TipoIntegracao, InstrucaoConexao[]> = {
  claude_cli: [
    { tipo: "mensagem", texto: "Abrindo o /login do Claude CLI (simulado)…" },
    { tipo: "mensagem", texto: "Conclua o login no navegador." },
  ],
  obsidian_cli: [],
  github: [
    {
      tipo: "codigo_do_dispositivo",
      codigo: "SIMU-1234",
      endereco: "https://github.com/login/device",
    },
  ],
};

/** O que a conexão simulada grava ao concluir. */
const CONCLUSAO = {
  claude_cli: { versao: "2.1.0 (Claude Code) [simulado]" },
  obsidian_cli: { notas_indexadas: 128 },
  github: { conta: "conta-simulada" },
} satisfies Record<TipoIntegracao, Partial<Omit<Integracao, "tipo">>>;

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

/**
 * ERR-53 e ERR-55 valem para as três integrações. `sem login` e `não instalado` só para o
 * Claude CLI: o núcleo só manda esses estados para ele.
 */
function cenariosDeCadaIntegracao(): Record<string, CenarioDeIntegracoes> {
  const cenarios: Record<string, CenarioDeIntegracoes> = {};
  for (const tipo of TIPOS) {
    const p = PREFIXO[tipo];
    const nome = NOME[tipo];
    if (tipo === "claude_cli") {
      cenarios[`${p}-sem-login`] = {
        descricao: `${nome} sem login`,
        estado: { [tipo]: { status: "sem_login", conta: null, versao: null } },
      };
      cenarios[`${p}-nao-instalado`] = {
        descricao: `${nome} não instalado, sem erro no estado`,
        estado: { [tipo]: { status: "nao_instalado", conta: null, versao: null } },
      };
    }
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

/** Os cenários das integrações, por nome (kebab-case). */
const CENARIOS: Record<string, CenarioDeIntegracoes> = {
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

export const funcionalidade = definirFuncionalidade({
  nome: "integracoes",
  padrao: "Claude CLI conectado; Obsidian CLI e GitHub desconectados",
  cenarios: CENARIOS,
  criar({ cenario, atrasoMs }) {
    const escolhido: CenarioDeIntegracoes = Object.hasOwn(CENARIOS, cenario)
      ? CENARIOS[cenario]!
      : { descricao: "padrão" };
    const inicial = { ...ESTADO_INICIAL_SIMULADO, ...escolhido.estado };
    const integracoes = criarNucleoSimuladoDeIntegracoes(inicial);
    if (escolhido.naoLigada) {
      integracoes.desligar(escolhido.naoLigada.tipo, escolhido.naoLigada.erro);
    }
    const base = integracoes.handlers;
    // Conexão em andamento por integração: "Conectar" de novo mostra a mesma instrução e
    // espera o mesmo fluxo (UC-01 FA-02, AC-157).
    const emAndamento = new Map<TipoIntegracao, Promise<Integracao>>();

    const conectarIntegracao: NonNullable<typeof base.conectarIntegracao> = (args) => {
      const { tipo, instrucoes } = args;
      // Integração não ligada (ERR-55): o núcleo falha antes de mandar instruções.
      if (integracoes.desligada(tipo)) return base.conectarIntegracao!(args);
      const atual = emAndamento.get(tipo);
      if (atual) {
        for (const instrucao of INSTRUCOES_SIMULADAS[tipo]) instrucoes.onmessage(instrucao);
        return atual;
      }
      if (tipo === "obsidian_cli") {
        integracoes.definir(tipo, CONCLUSAO.obsidian_cli);
        return base.conectarIntegracao!(args);
      }
      integracoes.roteiroDeConexao(tipo, {
        instrucoes: INSTRUCOES_SIMULADAS[tipo],
        aguardarHumano: true,
      });
      const fluxo = Promise.resolve(base.conectarIntegracao!(args)).finally(() =>
        emAndamento.delete(tipo),
      );
      emAndamento.set(tipo, fluxo);
      setTimeout(() => integracoes.concluirConexao(tipo, CONCLUSAO[tipo]), atrasoMs);
      return fluxo;
    };

    return { handlers: { ...base, conectarIntegracao }, controle: integracoes };
  },
});
