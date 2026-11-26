// Estado da tela das integrações (CMP-03 e CMP-11): o que o núcleo devolveu, a operação em
// andamento, o último erro e as instruções de conexão recebidas pelo canal.
import type { ErroCatalogo, InstrucaoConexao, Integracao, TipoIntegracao } from "@/lib/ipc";
import { ORDEM_DAS_INTEGRACOES, statusDepoisDoErro } from "./mensagens";

export type Operacao = "checar" | "conectar" | "desconectar";

export type EstadoDaIntegracao = {
  tipo: TipoIntegracao;
  /** `null` enquanto a primeira leitura não chegou. */
  integracao: Integracao | null;
  emAndamento: Operacao | null;
  erro: { erro: ErroCatalogo; operacao: Operacao } | null;
  instrucoes: InstrucaoConexao[];
};

export type EstadoDasIntegracoes = Record<TipoIntegracao, EstadoDaIntegracao>;

export type Acao =
  | { tipo: "iniciou"; integracao: TipoIntegracao; operacao: Operacao }
  | { tipo: "recebeu"; integracao: Integracao }
  /** Evento `integracao-mudou` do núcleo: não interrompe uma operação em andamento. */
  | { tipo: "mudou"; integracao: Integracao }
  | { tipo: "instrucao"; integracao: TipoIntegracao; instrucao: InstrucaoConexao }
  | { tipo: "falhou"; integracao: TipoIntegracao; operacao: Operacao; erro: ErroCatalogo };

export function estadoInicial(): EstadoDasIntegracoes {
  const estado = {} as EstadoDasIntegracoes;
  for (const tipo of ORDEM_DAS_INTEGRACOES) {
    estado[tipo] = { tipo, integracao: null, emAndamento: null, erro: null, instrucoes: [] };
  }
  return estado;
}

export function reduzir(estado: EstadoDasIntegracoes, acao: Acao): EstadoDasIntegracoes {
  switch (acao.tipo) {
    case "iniciou": {
      const atual = estado[acao.integracao];
      return {
        ...estado,
        [acao.integracao]: {
          ...atual,
          emAndamento: acao.operacao,
          instrucoes: acao.operacao === "conectar" ? [] : atual.instrucoes,
        },
      };
    }
    case "recebeu": {
      // O estado do núcleo traz o erro que o explica (M-46), inclusive o de `listar`: o
      // cartão mostra o motivo como se a checagem tivesse acabado de falhar.
      const tipo = acao.integracao.tipo;
      return {
        ...estado,
        [tipo]: {
          ...estado[tipo],
          integracao: acao.integracao,
          emAndamento: null,
          erro: acao.integracao.erro ? { erro: acao.integracao.erro, operacao: "checar" } : null,
          instrucoes: [],
        },
      };
    }
    case "mudou": {
      const atual = estado[acao.integracao.tipo];
      if (atual.emAndamento === null) return reduzir(estado, { ...acao, tipo: "recebeu" });
      return { ...estado, [acao.integracao.tipo]: { ...atual, integracao: acao.integracao } };
    }
    case "instrucao": {
      const atual = estado[acao.integracao];
      return {
        ...estado,
        [acao.integracao]: { ...atual, instrucoes: [...atual.instrucoes, acao.instrucao] },
      };
    }
    case "falhou": {
      const atual = estado[acao.integracao];
      const status = statusDepoisDoErro(acao.erro);
      const base: Integracao = atual.integracao ?? {
        tipo: acao.integracao,
        status: "desconectado",
        conta: null,
        versao: null,
        notas_indexadas: null,
        erro: null,
      };
      const integracao: Integracao | null =
        status === null
          ? atual.integracao
          : {
              ...base,
              status,
              conta: null,
              versao: status === "nao_instalado" ? null : base.versao,
              notas_indexadas: null,
              erro: acao.erro,
            };
      return {
        ...estado,
        [acao.integracao]: {
          ...atual,
          integracao,
          emAndamento: null,
          erro: { erro: acao.erro, operacao: acao.operacao },
          instrucoes: [],
        },
      };
    }
  }
}

/** Situação mostrada no CMP-03 e no CMP-11. */
export type Situacao = "carregando" | "conectado" | "erro";

export function situacao(e: EstadoDaIntegracao): Situacao {
  if (e.emAndamento !== null) return "carregando";
  if (e.erro !== null) return "erro";
  if (e.integracao === null) return "carregando";
  return e.integracao.status === "conectado" ? "conectado" : "erro";
}
