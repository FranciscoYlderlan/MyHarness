// Porta única da tela para o núcleo: os comandos e eventos tipados de `src/bindings/`
// (gerados do Rust), com validação zod de tudo o que chega por evento, canal ou resposta
// de comando.
import { Channel } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { z } from "zod";
import { commands, events } from "@/bindings";
import type { ErroIpc, EventoExecucao, InstrucaoConexao } from "@/bindings";
import { erroCatalogo, esquemasDeEvento, eventoExecucao, instrucaoConexao } from "./esquemas";

export { commands };
export type * from "@/bindings";

/** Nome de um evento global do núcleo (chave de `events`). */
export type NomeEvento = keyof typeof esquemasDeEvento;

/** Payload já validado do evento `N`. */
export type PayloadDe<N extends NomeEvento> = z.infer<(typeof esquemasDeEvento)[N]>;

/** Quem recebe um payload que não passou na validação (padrão: `console.error`). */
export type AoInvalido = (origem: string, erro: z.ZodError, bruto: unknown) => void;

const relatarInvalido: AoInvalido = (origem, erro, bruto) => {
  console.error(`payload inválido do núcleo em ${origem}`, erro.issues, bruto);
};

/**
 * Ouve um evento global do núcleo. Payload fora do contrato não chega ao `aoReceber`:
 * vai para `aoInvalido`.
 */
export function ouvir<N extends NomeEvento>(
  nome: N,
  aoReceber: (payload: PayloadDe<N>) => void,
  aoInvalido: AoInvalido = relatarInvalido,
): Promise<UnlistenFn> {
  const esquema = esquemasDeEvento[nome];
  const evento = events[nome] as {
    listen: (cb: (e: { payload: unknown }) => void) => Promise<UnlistenFn>;
  };
  return evento.listen(({ payload }) => {
    const r = esquema.safeParse(payload);
    if (r.success) aoReceber(r.data as PayloadDe<N>);
    else aoInvalido(nome, r.error, payload);
  });
}

function canalValidado<T>(
  origem: string,
  esquema: z.ZodType<T>,
  aoReceber: (valor: T) => void,
  aoInvalido: AoInvalido,
): Channel<T> {
  const canal = new Channel<T>();
  canal.onmessage = (bruto: unknown) => {
    const r = esquema.safeParse(bruto);
    if (r.success) aoReceber(r.data);
    else aoInvalido(origem, r.error, bruto);
  };
  return canal;
}

/** Canal da saída ao vivo de uma execução (`enviarPrompt`, `acompanharExecucao`). */
export function canalDeExecucao(
  aoReceber: (evento: EventoExecucao) => void,
  aoInvalido: AoInvalido = relatarInvalido,
): Channel<EventoExecucao> {
  return canalValidado("canal de execução", eventoExecucao, aoReceber, aoInvalido);
}

/** Canal das instruções de conexão de uma integração (`conectarIntegracao`). */
export function canalDeConexao(
  aoReceber: (instrucao: InstrucaoConexao) => void,
  aoInvalido: AoInvalido = relatarInvalido,
): Channel<InstrucaoConexao> {
  return canalValidado("canal de conexão", instrucaoConexao, aoReceber, aoInvalido);
}

/** Resposta de um comando, como os bindings devolvem. */
export type RespostaDeComando<T> = { status: "ok"; data: T } | { status: "error"; error: ErroIpc };

/** ERR-55 da resposta de comando fora do contrato. */
const RESPOSTA_FORA_DO_CONTRATO: ErroIpc = {
  tipo: "catalogo",
  erro: {
    codigo: "FALHA_INTERNA",
    id: "ERR-55",
    parametros: { motivo: "a resposta do núcleo está fora do contrato" },
  },
};

/**
 * Valida pelo zod a resposta de um comando: o dado com `esquema` e o erro com o catálogo.
 * Resposta fora do contrato não chega à tela: vai para `aoInvalido` e vira o ERR-55.
 */
export function validarResposta<T>(
  origem: string,
  esquema: z.ZodType<T>,
  resposta: { status: "ok"; data: unknown } | { status: "error"; error: unknown },
  aoInvalido: AoInvalido = relatarInvalido,
): RespostaDeComando<T> {
  if (resposta.status === "ok") {
    const r = esquema.safeParse(resposta.data);
    if (r.success) return { status: "ok", data: r.data };
    aoInvalido(origem, r.error, resposta.data);
    return { status: "error", error: RESPOSTA_FORA_DO_CONTRATO };
  }
  const bruto = resposta.error as { tipo?: unknown; erro?: unknown } | null;
  const r = erroCatalogo.safeParse(bruto?.tipo === "catalogo" ? bruto.erro : undefined);
  if (r.success) return { status: "error", error: { tipo: "catalogo", erro: r.data } };
  aoInvalido(origem, r.error, resposta.error);
  return { status: "error", error: RESPOSTA_FORA_DO_CONTRATO };
}
