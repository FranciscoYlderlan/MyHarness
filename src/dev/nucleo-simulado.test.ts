// O roteiro do `dev:simulado` sobre o mesmo núcleo simulado dos testes da TEL-02.
import type { Channel } from "@tauri-apps/api/core";
import type { InstrucaoConexao } from "@/bindings";
import { INSTRUCOES_SIMULADAS } from "./funcionalidades/integracoes";
import { FUNCIONALIDADES_SIMULADAS, juntarCenarios } from "./cenarios";
import type { FuncionalidadeSimulada } from "./funcionalidade-simulada";
import { criarNucleoSimulado, juntarNucleos } from "./nucleo-simulado";

function canal() {
  const recebidas: InstrucaoConexao[] = [];
  const instrucoes = {
    onmessage: (i: InstrucaoConexao) => recebidas.push(i),
  } as unknown as Channel<InstrucaoConexao>;
  return { instrucoes, recebidas };
}

afterEach(() => vi.useRealTimers());

describe("núcleo do dev:simulado", () => {
  it("lista as três integrações", async () => {
    const { handlers } = criarNucleoSimulado();
    const lista = await handlers.listarIntegracoes!();
    expect(lista.map((i) => [i.tipo, i.status])).toEqual([
      ["claude_cli", "conectado"],
      ["obsidian_cli", "desconectado"],
      ["github", "desconectado"],
    ]);
  });

  it("conecta o GitHub pelo código do device flow e conclui depois do atraso", async () => {
    vi.useFakeTimers();
    const { handlers } = criarNucleoSimulado({ atrasoMs: 1000 });
    const primeiro = canal();
    const fluxo = handlers.conectarIntegracao!({ tipo: "github", ...primeiro });
    expect(primeiro.recebidas).toEqual(INSTRUCOES_SIMULADAS.github);

    // "Conectar" de novo: a mesma instrução e o mesmo fluxo (UC-01 FA-02).
    const segundo = canal();
    const repetido = handlers.conectarIntegracao!({ tipo: "github", ...segundo });
    expect(segundo.recebidas).toEqual(INSTRUCOES_SIMULADAS.github);
    expect(repetido).toBe(fluxo);

    await vi.advanceTimersByTimeAsync(1000);
    await expect(fluxo).resolves.toMatchObject({ status: "conectado", conta: "conta-simulada" });
  });

  it("conecta a Obsidian CLI na hora, com o número de notas", async () => {
    const { handlers } = criarNucleoSimulado();
    const resultado = await handlers.conectarIntegracao!({ tipo: "obsidian_cli", ...canal() });
    expect(resultado).toMatchObject({ status: "conectado", notas_indexadas: 128 });
  });
});

describe("composição por funcionalidade", () => {
  const vazia = (nome: string, cenarios: Record<string, { descricao: string }> = {}) =>
    ({
      nome,
      padrao: `${nome} vazio`,
      cenarios,
      criar: () => ({ handlers: {}, controle: undefined }),
    }) as unknown as FuncionalidadeSimulada;

  it("junta os arquivos de src/dev/funcionalidades/, com integracoes entre eles", () => {
    expect(FUNCIONALIDADES_SIMULADAS.map((f) => f.nome)).toContain("integracoes");
    const nucleo = criarNucleoSimulado();
    expect(nucleo.integracoes.estado("claude_cli").status).toBe("conectado");
    expect(nucleo.handlers.listarIntegracoes).toBeTypeOf("function");
  });

  it("o padrão descreve o estado padrão de cada funcionalidade", () => {
    const cenarios = juntarCenarios([
      vazia("a"),
      vazia("b", { "b-cheio": { descricao: "cheio" } }),
    ]);
    expect(cenarios).toEqual({
      padrao: { descricao: "a: a vazio · b: b vazio", funcionalidade: null },
      "b-cheio": { descricao: "cheio", funcionalidade: "b" },
    });
  });

  it("cenário com o mesmo nome em duas funcionalidades é erro", () => {
    const x = { x: { descricao: "x" } };
    expect(() => juntarCenarios([vazia("a", x), vazia("b", x)])).toThrow(
      'cenário "x" repetido: b e a',
    );
    expect(() => juntarCenarios([vazia("a", { padrao: { descricao: "p" } })])).toThrow(
      'cenário "padrao" repetido',
    );
  });

  it("comando com handler em duas funcionalidades é erro", () => {
    const comLista = (nome: string) =>
      ({
        ...vazia(nome),
        criar: () => ({ handlers: { listarIntegracoes: () => [] }, controle: undefined }),
      }) as unknown as FuncionalidadeSimulada;
    expect(() =>
      juntarNucleos([comLista("a"), comLista("b")], { cenario: "padrao", atrasoMs: 0 }),
    ).toThrow('comando "listarIntegracoes" repetido no núcleo simulado: a e b');
  });

  it("cenário desconhecido é erro", () => {
    expect(() => criarNucleoSimulado({ cenario: "nao-existe" })).toThrow("cenário desconhecido");
  });
});
