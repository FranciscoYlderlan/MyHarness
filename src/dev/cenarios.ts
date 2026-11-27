// Cenários do modo simulado (`pnpm dev:simulado`, `?cenario=<nome>` na URL): o estado em
// que o núcleo simulado abre a tela. Cada funcionalidade declara os seus no próprio arquivo
// (`./funcionalidades/<nome>.ts`); aqui eles só se juntam. Os testes do vitest abrem a tela
// em cada um.
import type { FuncionalidadeSimulada } from "./funcionalidade-simulada";

/** Os arquivos de funcionalidade do modo simulado, em ordem de nome. */
export const FUNCIONALIDADES_SIMULADAS: FuncionalidadeSimulada[] = Object.entries(
  import.meta.glob<FuncionalidadeSimulada>(
    ["./funcionalidades/*.ts", "!./funcionalidades/*.test.ts"],
    { eager: true, import: "funcionalidade" },
  ),
)
  .sort(([a], [b]) => a.localeCompare(b))
  .map(([, f]) => f);

export type Cenario = {
  /** O que a tela mostra ao abrir, em uma frase. */
  descricao: string;
  /** Dona do cenário; `null` no padrão, que vale para todas. */
  funcionalidade: string | null;
};

export const CENARIO_PADRAO = "padrao";

/**
 * Junta os cenários das funcionalidades. Nome repetido entre duas funcionalidades, ou igual
 * ao padrão, é erro: o `?cenario=` tem de apontar uma dona só.
 */
export function juntarCenarios(funcionalidades: FuncionalidadeSimulada[]): Record<string, Cenario> {
  const cenarios: Record<string, Cenario> = {
    [CENARIO_PADRAO]: {
      descricao: funcionalidades.map((f) => `${f.nome}: ${f.padrao}`).join(" · "),
      funcionalidade: null,
    },
  };
  for (const f of funcionalidades) {
    for (const [nome, { descricao }] of Object.entries(f.cenarios)) {
      if (Object.hasOwn(cenarios, nome)) {
        const dona = cenarios[nome]!.funcionalidade ?? "o padrão";
        throw new Error(`cenário "${nome}" repetido: ${f.nome} e ${dona}`);
      }
      cenarios[nome] = { descricao, funcionalidade: f.nome };
    }
  }
  return cenarios;
}

/** Os cenários do `dev:simulado`, por nome (kebab-case). */
export const CENARIOS: Record<string, Cenario> = juntarCenarios(FUNCIONALIDADES_SIMULADAS);

/** Os nomes dos cenários de uma funcionalidade (sem o padrão). */
export function cenariosDa(funcionalidade: string): string[] {
  return Object.entries(CENARIOS)
    .filter(([, c]) => c.funcionalidade === funcionalidade)
    .map(([nome]) => nome);
}

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
