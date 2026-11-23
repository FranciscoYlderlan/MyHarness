// Modo simulado (`pnpm dev:simulado`, só desenvolvimento): a tela roda no navegador com o
// núcleo simulado das integrações, sem Claude CLI, Obsidian nem GitHub. Usa o mesmo núcleo
// simulado dos testes do vitest (`@/lib/ipc/nucleo-simulado-integracoes`); aqui só entra
// o roteiro de uma sessão de desenvolvimento. Não entra no app instalado: o Vite só carrega
// este arquivo no `vite --mode simulado` (ver `vite.config.ts`).
import type { InstrucaoConexao, Integracao, MockCommandHandlers, TipoIntegracao } from "@/bindings";
import { mockCommands } from "@/bindings";
import { CENARIO_PADRAO, CENARIOS, ESTADO_INICIAL_SIMULADO } from "./cenarios";
import {
  criarNucleoSimuladoDeIntegracoes,
  type NucleoSimuladoDeIntegracoes,
} from "@/lib/ipc/nucleo-simulado-integracoes";

export { ESTADO_INICIAL_SIMULADO } from "./cenarios";

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

export type OpcoesDoNucleoSimulado = {
  /** Tempo até o "Humano" concluir o login (Claude CLI e GitHub), em ms. */
  atrasoMs?: number;
  /** Nome em `CENARIOS` do estado em que o núcleo abre; sem ele, o padrão. */
  cenario?: string;
};

export type NucleoSimulado = {
  /** O núcleo simulado das integrações, para mudar o estado à mão (console, Playwright). */
  integracoes: NucleoSimuladoDeIntegracoes;
  /** Handlers para `mockCommands`. */
  handlers: Partial<MockCommandHandlers>;
};

export function criarNucleoSimulado({
  atrasoMs = 3000,
  cenario = CENARIO_PADRAO,
}: OpcoesDoNucleoSimulado = {}): NucleoSimulado {
  const escolhido = CENARIOS[cenario];
  if (!escolhido) throw new Error(`cenário desconhecido no núcleo simulado: ${cenario}`);
  const inicial = { ...ESTADO_INICIAL_SIMULADO, ...escolhido.estado };
  const integracoes = criarNucleoSimuladoDeIntegracoes(inicial);
  if (escolhido.naoLigada) integracoes.desligar(escolhido.naoLigada.tipo, escolhido.naoLigada.erro);
  const base = integracoes.handlers;
  // Conexão em andamento por integração: "Conectar" de novo mostra a mesma instrução e
  // espera o mesmo fluxo (UC-01 FA-02, AC-157).
  const emAndamento = new Map<TipoIntegracao, Promise<Integracao>>();

  const conectarIntegracao: MockCommandHandlers["conectarIntegracao"] = (args) => {
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

  return { integracoes, handlers: { ...base, conectarIntegracao } };
}

declare global {
  interface Window {
    /** Núcleo simulado do `dev:simulado`, para mudar o estado pelo console. */
    __nucleoSimulado?: NucleoSimulado;
  }
}

/** Instala o núcleo simulado no lugar do IPC do Tauri. */
export function instalarNucleoSimulado(opcoes?: OpcoesDoNucleoSimulado): NucleoSimulado {
  const nucleo = criarNucleoSimulado(opcoes);
  mockCommands(nucleo.handlers);
  window.__nucleoSimulado = nucleo;
  console.info(
    `[MyHarness] modo simulado: núcleo simulado das integrações instalado (cenário "${opcoes?.cenario ?? CENARIO_PADRAO}")`,
  );
  return nucleo;
}
