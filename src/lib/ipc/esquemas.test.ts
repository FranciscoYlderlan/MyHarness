import type { z } from "zod";
import type * as B from "@/bindings";
import * as E from "./esquemas";

// Igualdade de tipos nos dois sentidos: o schema zod e o tipo gerado do Rust não podem
// divergir (campo a mais, a menos ou valor de enumeração diferente não compila).
type Igual<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2 ? true : false;
function mesmoTipo<T extends true>(): T | undefined {
  return undefined;
}

mesmoTipo<Igual<z.infer<typeof E.codigoErro>, B.CodigoErro>>();
mesmoTipo<Igual<z.infer<typeof E.erroCatalogo>, B.ErroCatalogo>>();
mesmoTipo<Igual<z.infer<typeof E.integracao>, B.Integracao>>();
mesmoTipo<Igual<z.infer<typeof E.instrucaoConexao>, B.InstrucaoConexao>>();
mesmoTipo<Igual<z.infer<typeof E.consumo>, B.Consumo>>();
mesmoTipo<Igual<z.infer<typeof E.chat>, B.Chat>>();
mesmoTipo<Igual<z.infer<typeof E.mensagem>, B.Mensagem>>();
mesmoTipo<Igual<z.infer<typeof E.execucao>, B.Execucao>>();
mesmoTipo<Igual<z.infer<typeof E.log>, B.Log>>();
mesmoTipo<Igual<z.infer<typeof E.leitura>, B.Leitura>>();
mesmoTipo<Igual<z.infer<typeof E.pendencia>, B.Pendencia>>();
mesmoTipo<Igual<z.infer<typeof E.criterio>, B.Criterio>>();
mesmoTipo<Igual<z.infer<typeof E.relatorio>, B.Relatorio>>();
mesmoTipo<Igual<z.infer<typeof E.eventoExecucao>, B.EventoExecucao>>();
mesmoTipo<Igual<z.infer<typeof E.esquemasDeEvento.integracaoMudou>, B.IntegracaoMudou>>();
mesmoTipo<Igual<z.infer<typeof E.esquemasDeEvento.consumoAtualizado>, B.ConsumoAtualizado>>();
mesmoTipo<Igual<z.infer<typeof E.esquemasDeEvento.chatMudou>, B.ChatMudou>>();
mesmoTipo<Igual<z.infer<typeof E.esquemasDeEvento.execucaoMudou>, B.ExecucaoMudou>>();
mesmoTipo<Igual<z.infer<typeof E.esquemasDeEvento.logRegistrado>, B.LogRegistrado>>();
mesmoTipo<Igual<z.infer<typeof E.esquemasDeEvento.leituraRegistrada>, B.LeituraRegistrada>>();
mesmoTipo<Igual<z.infer<typeof E.esquemasDeEvento.pendenciasMudaram>, B.PendenciasMudaram>>();

describe("esquemas do contrato IPC", () => {
  it("aceita só data e hora em UTC com Z", () => {
    for (const ok of [
      "2026-11-19T18:20:00Z",
      "2026-11-19T18:20:00.123Z",
      "2026-11-19T18:20:00.5Z",
      "2026-11-19T18:20:00.123456789Z",
    ]) {
      expect(E.dataHoraUtc.safeParse(ok).success).toBe(true);
    }
    for (const ruim of [
      "2026-11-19T18:20:00",
      "2026-11-19T18:20:00-03:00",
      "2026-11-19T18:20:00+00:00",
      "2026-11-19",
      "19/11/2026 18:20",
      "2026-11-19T18:20Z",
      "2026-11-19T18:20:00.Z",
      "2026-11-19T18:20:00.1234567890Z",
    ]) {
      expect(E.dataHoraUtc.safeParse(ruim).success).toBe(false);
    }
  });

  it("valida o log com datas e recusa hora local", () => {
    const log: B.Log = {
      id: "1",
      execucao: "e1",
      comando: "git status",
      inicio: "2026-11-19T18:20:00Z",
      fim: null,
      codigo_saida: null,
      saida_mascarada: null,
    };
    expect(E.log.parse(log)).toEqual(log);
    expect(E.log.safeParse({ ...log, inicio: "2026-11-19T15:20:00-03:00" }).success).toBe(false);
  });

  it("recusa código de erro fora do catálogo", () => {
    const erro: B.ErroCatalogo = { codigo: "SUBAGENTE_CAIU", id: "ERR-17", parametros: {} };
    expect(E.erroCatalogo.parse(erro)).toEqual(erro);
    expect(E.erroCatalogo.safeParse({ ...erro, codigo: "OUTRO" }).success).toBe(false);
    expect(E.codigoErro.options).toHaveLength(54);
  });

  it("valida cada variante da saída ao vivo", () => {
    const eventos: B.EventoExecucao[] = [
      { tipo: "texto", texto: "olá" },
      { tipo: "ferramenta", nome: "Read", resumo: "src/main.tsx" },
      { tipo: "uso_tokens", tokens_usados: 120 },
      { tipo: "plano_proposto", plano: "p1" },
      { tipo: "status", status: "concluido" },
      { tipo: "erro", erro: { codigo: "SUBAGENTE_CAIU", id: "ERR-17", parametros: {} } },
      {
        tipo: "mensagem",
        mensagem: { id: "m1", chat: "c1", autor: "orquestrador", texto: "ok", uso_tokens: 10 },
      },
    ];
    for (const e of eventos) expect(E.eventoExecucao.parse(e)).toEqual(e);
    expect(E.eventoExecucao.safeParse({ tipo: "status", status: "pronto" }).success).toBe(false);
    expect(E.eventoExecucao.safeParse({ tipo: "uso_tokens", tokens_usados: -1 }).success).toBe(
      false,
    );
  });
});
