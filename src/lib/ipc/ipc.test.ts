import { emit } from "@tauri-apps/api/event";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mockCommands } from "@/bindings";
import type { ChatMudou, EventoExecucao } from "@/bindings";
import { canalDeExecucao, commands, ouvir, validarResposta } from ".";
import { integracao } from "./esquemas";

afterEach(() => clearMocks());

describe("porta IPC", () => {
  it("comando devolve o resultado tipado e o erro do catálogo", async () => {
    mockCommands({
      listarIntegracoes: () => [
        {
          tipo: "claude_cli",
          status: "desconectado",
          conta: null,
          versao: null,
          notas_indexadas: null,
          erro: null,
        },
      ],
      lerConsumo: () => {
        throw {
          tipo: "catalogo",
          erro: { codigo: "CONSUMO_INDISPONIVEL", id: "ERR-41", parametros: {} },
        };
      },
    });

    const integracoes = await commands.listarIntegracoes();
    expect(integracoes).toEqual({
      status: "ok",
      data: [
        {
          tipo: "claude_cli",
          status: "desconectado",
          conta: null,
          versao: null,
          notas_indexadas: null,
          erro: null,
        },
      ],
    });

    const consumo = await commands.lerConsumo();
    expect(consumo.status).toBe("error");
    if (consumo.status === "error" && consumo.error.tipo === "catalogo") {
      expect(consumo.error.erro.codigo).toBe("CONSUMO_INDISPONIVEL");
    }
  });

  it("evento válido chega; evento fora do contrato vai para aoInvalido", async () => {
    mockCommands({});
    const recebidos: ChatMudou[] = [];
    const invalidos: unknown[] = [];
    const parar = await ouvir(
      "chatMudou",
      (p) => recebidos.push(p),
      (_origem, _erro, bruto) => invalidos.push(bruto),
    );

    const valido: ChatMudou = {
      chat: {
        id: "c1",
        projeto: "p1",
        limite_tokens: 100000,
        tokens_usados: 100500,
        chat_anterior: null,
        status: "ativo",
        continuacoes: 0,
      },
      avisos: [{ codigo: "LIMITE_DO_CHAT_ATINGIDO", id: "ERR-48", parametros: {} }],
    };
    await emit("chat-mudou", valido);
    await emit("chat-mudou", { chat: { ...valido.chat, status: "outro" }, avisos: [] });
    parar();

    expect(recebidos).toEqual([valido]);
    expect(invalidos).toHaveLength(1);
  });

  it("canal de execução entrega só mensagens válidas", () => {
    const recebidos: EventoExecucao[] = [];
    const invalidos: unknown[] = [];
    mockCommands({});
    const canal = canalDeExecucao(
      (e) => recebidos.push(e),
      (_o, _e, bruto) => invalidos.push(bruto),
    );
    canal.onmessage({ tipo: "texto", texto: "olá" });
    canal.onmessage({ tipo: "texto" } as unknown as EventoExecucao);
    expect(recebidos).toEqual([{ tipo: "texto", texto: "olá" }]);
    expect(invalidos).toHaveLength(1);
  });

  it("resposta de comando passa pelo zod: fora do contrato vira o ERR-55", () => {
    const valida = {
      tipo: "obsidian_cli",
      status: "conectado",
      conta: null,
      versao: null,
      notas_indexadas: 75,
      erro: null,
    };
    const invalidos: unknown[] = [];
    const aoInvalido = (_origem: string, _erro: unknown, bruto: unknown) => invalidos.push(bruto);
    const err55 = {
      status: "error",
      error: {
        tipo: "catalogo",
        erro: {
          codigo: "FALHA_INTERNA",
          id: "ERR-55",
          parametros: { motivo: "a resposta do núcleo está fora do contrato" },
        },
      },
    };

    expect(
      validarResposta("testar", integracao, { status: "ok", data: valida }, aoInvalido),
    ).toEqual({ status: "ok", data: valida });
    const negativa = { ...valida, notas_indexadas: -5 };
    expect(
      validarResposta("testar", integracao, { status: "ok", data: negativa }, aoInvalido),
    ).toEqual(err55);

    const erro = {
      tipo: "catalogo",
      erro: { codigo: "OBSIDIAN_FECHADO", id: "ERR-03", parametros: {} },
    };
    expect(
      validarResposta("testar", integracao, { status: "error", error: erro }, aoInvalido),
    ).toEqual({ status: "error", error: erro });
    const erroForaDoCatalogo = { tipo: "catalogo", erro: { codigo: "INVENTADO", id: "ERR-99" } };
    expect(
      validarResposta(
        "testar",
        integracao,
        { status: "error", error: erroForaDoCatalogo },
        aoInvalido,
      ),
    ).toEqual(err55);
    expect(invalidos).toEqual([negativa, erroForaDoCatalogo]);
  });
});
