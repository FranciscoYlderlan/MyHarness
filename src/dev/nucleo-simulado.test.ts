// O roteiro do `dev:simulado` sobre o mesmo núcleo simulado dos testes da TEL-02.
import type { Channel } from "@tauri-apps/api/core";
import type { InstrucaoConexao } from "@/bindings";
import { criarNucleoSimulado, INSTRUCOES_SIMULADAS } from "./nucleo-simulado";

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
