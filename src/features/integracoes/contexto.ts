import { createContext, useContext } from "react";
import type { TipoIntegracao } from "@/lib/ipc";
import type { EstadoDasIntegracoes } from "./estado";

export type Integracoes = {
  estado: EstadoDasIntegracoes;
  /** Checa o status de uma integração (`testar_integracao`). */
  checar: (tipo: TipoIntegracao) => void;
  /** Checa as três (abertura do app e da TEL-02). */
  checarTodas: () => void;
  /** Inicia o `/login`, o device flow ou o teste do servidor MCP (UC-01 passo 2). */
  conectar: (tipo: TipoIntegracao) => void;
  desconectar: (tipo: TipoIntegracao) => void;
  /** Repete a operação que falhou por último (tratamento `repetir` do catálogo). */
  repetir: (tipo: TipoIntegracao) => void;
};

export const ContextoDasIntegracoes = createContext<Integracoes | null>(null);

export function useIntegracoes(): Integracoes {
  const valor = useContext(ContextoDasIntegracoes);
  if (!valor) throw new Error("useIntegracoes fora do ProvedorDeIntegracoes");
  return valor;
}
