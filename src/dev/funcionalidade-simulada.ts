// O que cada funcionalidade entrega ao modo simulado (`pnpm dev:simulado`): os cenários
// dela e os handlers dos comandos dela. Cada funcionalidade mora num arquivo próprio em
// `./funcionalidades/<nome>.ts`; `./cenarios` e `./nucleo-simulado` juntam todos sozinhos
// (`import.meta.glob`), então uma funcionalidade nova não edita nenhum arquivo comum.
import type { MockCommandHandlers } from "@/bindings";

/**
 * Controles do núcleo simulado, por funcionalidade, para mudar o estado à mão (console,
 * Playwright: `window.__nucleoSimulado.<nome>`). Cada arquivo de funcionalidade acrescenta
 * a sua chave por `declare module "@/dev/funcionalidade-simulada"`.
 */
// eslint-disable-next-line @typescript-eslint/no-empty-object-type -- aberta para declaration merging
export interface ControlesSimulados {}

export type OpcoesDaFuncionalidade = {
  /**
   * Nome do cenário pedido (de qualquer funcionalidade). Cenário de outra funcionalidade:
   * esta abre no estado padrão dela.
   */
  cenario: string;
  /** Tempo até o "Humano" concluir uma ação simulada (login, por exemplo), em ms. */
  atrasoMs: number;
};

export type FuncionalidadeSimulada<N extends keyof ControlesSimulados = keyof ControlesSimulados> =
  {
    /** Chave do controle em `window.__nucleoSimulado` (nome da pasta em `src/features/`). */
    nome: N;
    /** O que o estado padrão desta funcionalidade mostra, em uma frase. */
    padrao: string;
    /** Cenários próprios, por nome em kebab-case, únicos entre as funcionalidades. */
    cenarios: Record<string, { descricao: string }>;
    criar(opcoes: OpcoesDaFuncionalidade): {
      /** Só os comandos desta funcionalidade; dois arquivos com o mesmo comando é erro. */
      handlers: Partial<MockCommandHandlers>;
      controle: ControlesSimulados[N];
    };
  };

/** Ajuda a tipar o arquivo de uma funcionalidade. */
export function definirFuncionalidade<N extends keyof ControlesSimulados>(
  funcionalidade: FuncionalidadeSimulada<N>,
): FuncionalidadeSimulada<N> {
  return funcionalidade;
}
