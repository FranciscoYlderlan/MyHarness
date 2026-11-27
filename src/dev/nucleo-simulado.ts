// Modo simulado (`pnpm dev:simulado`, só desenvolvimento): a tela roda no navegador com o
// núcleo simulado, sem Claude CLI, Obsidian nem GitHub. Cada funcionalidade entrega os
// próprios handlers e o próprio controle (`./funcionalidades/<nome>.ts`); aqui eles só se
// juntam. Não entra no app instalado: o Vite só carrega este arquivo no
// `vite --mode simulado` (ver `vite.config.ts`).
import type { MockCommandHandlers } from "@/bindings";
import { mockCommands } from "@/bindings";
import { CENARIO_PADRAO, CENARIOS, FUNCIONALIDADES_SIMULADAS } from "./cenarios";
import type { ControlesSimulados, FuncionalidadeSimulada } from "./funcionalidade-simulada";

export type OpcoesDoNucleoSimulado = {
  /** Tempo até o "Humano" concluir uma ação simulada (login, por exemplo), em ms. */
  atrasoMs?: number;
  /** Nome em `CENARIOS` do estado em que o núcleo abre; sem ele, o padrão. */
  cenario?: string;
};

/** Os controles de cada funcionalidade (`integracoes`, …) e os handlers juntos. */
export type NucleoSimulado = ControlesSimulados & {
  /** Handlers para `mockCommands`. */
  handlers: Partial<MockCommandHandlers>;
};

/**
 * Junta os handlers e os controles das funcionalidades. Dois arquivos com o mesmo comando é
 * erro: cada comando tem uma dona só.
 */
export function juntarNucleos(
  funcionalidades: FuncionalidadeSimulada[],
  opcoes: { cenario: string; atrasoMs: number },
): NucleoSimulado {
  const handlers: Record<string, unknown> = {};
  const donos: Record<string, string> = {};
  const controles: Record<string, unknown> = {};
  for (const f of funcionalidades) {
    const criado = f.criar(opcoes);
    for (const [comando, handler] of Object.entries(criado.handlers)) {
      if (Object.hasOwn(handlers, comando)) {
        throw new Error(
          `comando "${comando}" repetido no núcleo simulado: ${donos[comando]} e ${f.nome}`,
        );
      }
      handlers[comando] = handler;
      donos[comando] = f.nome;
    }
    controles[f.nome] = criado.controle;
  }
  return {
    ...(controles as unknown as ControlesSimulados),
    handlers: handlers as Partial<MockCommandHandlers>,
  };
}

export function criarNucleoSimulado({
  atrasoMs = 3000,
  cenario = CENARIO_PADRAO,
}: OpcoesDoNucleoSimulado = {}): NucleoSimulado {
  if (!Object.hasOwn(CENARIOS, cenario)) {
    throw new Error(`cenário desconhecido no núcleo simulado: ${cenario}`);
  }
  return juntarNucleos(FUNCIONALIDADES_SIMULADAS, { cenario, atrasoMs });
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
    `[MyHarness] modo simulado: núcleo simulado instalado (${FUNCIONALIDADES_SIMULADAS.map((f) => f.nome).join(", ")}; cenário "${opcoes?.cenario ?? CENARIO_PADRAO}")`,
  );
  return nucleo;
}
