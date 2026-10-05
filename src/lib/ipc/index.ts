// Porta única da tela para o núcleo: os comandos e eventos tipados de `src/bindings/`
// (gerados do Rust), com validação zod de tudo o que chega por evento ou canal.
import { Channel } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { z } from "zod";
import { commands, events } from "@/bindings";
import type { EventoExecucao, InstrucaoConexao } from "@/bindings";
import { esquemasDeEvento, eventoExecucao, instrucaoConexao } from "./esquemas";

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
