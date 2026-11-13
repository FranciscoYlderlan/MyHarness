// Núcleo simulado das integrações (modo simulado, só desenvolvimento e teste): responde
// `listar_integracoes`, `testar_integracao`, `conectar_integracao` e `desconectar_integracao`
// pelo mesmo contrato IPC dos bindings, sem Claude CLI, Obsidian nem GitHub.
import type {
  ErroCatalogo,
  ErroIpc,
  InstrucaoConexao,
  Integracao,
  MockCommandHandlers,
  TipoIntegracao,
} from "@/bindings";

const ORDEM: TipoIntegracao[] = ["claude_cli", "obsidian_cli", "github"];

const NOMES: Record<TipoIntegracao, string> = {
  claude_cli: "Claude CLI",
  obsidian_cli: "Obsidian CLI",
  github: "GitHub",
};

type Pendente = {
  resolver: (i: Integracao) => void;
  rejeitar: (e: ErroIpc) => void;
};

/** Como o núcleo simulado responde a `conectar_integracao` de uma integração. */
export type RoteiroDeConexao = {
  /** Instruções mandadas pelo canal, na ordem (código do device flow, linhas do `/login`). */
  instrucoes: InstrucaoConexao[];
  /**
   * `true`: a conexão fica pendente até `concluirConexao` ou `falharConexao`, como o
   * Humano digitando o código no GitHub. Sem pendência, conclui na hora.
   */
  aguardarHumano?: boolean;
};

export type NucleoSimuladoDeIntegracoes = {
  /** Handlers para `mockCommands` dos bindings. */
  handlers: Partial<MockCommandHandlers>;
  /** Estado atual de uma integração no núcleo simulado. */
  estado: (tipo: TipoIntegracao) => Integracao;
  /** Troca o estado guardado (o que o próximo `testar` ou `listar` devolve). */
  definir: (tipo: TipoIntegracao, parcial: Partial<Omit<Integracao, "tipo">>) => void;
  /** A próxima checagem (`testar`) da integração falha com este erro; `null` limpa. */
  falharChecagem: (tipo: TipoIntegracao, erro: ErroIpc | null) => void;
  /** O próximo `conectar` da integração segue este roteiro. */
  roteiroDeConexao: (tipo: TipoIntegracao, roteiro: RoteiroDeConexao) => void;
  /** O próximo `conectar` falha com este erro, depois de mandar as instruções. */
  falharConexaoDireto: (tipo: TipoIntegracao, erro: ErroIpc | null) => void;
  /** Conclui a conexão pendente com `conectado` e a conta ou versão. */
  concluirConexao: (tipo: TipoIntegracao, dados: { conta?: string; versao?: string }) => void;
  /** Encerra a conexão pendente com erro. */
  falharConexao: (tipo: TipoIntegracao, erro: ErroIpc) => void;
  /** Próximo `desconectar` falha com este erro; `null` limpa. */
  falharDesconexao: (tipo: TipoIntegracao, erro: ErroIpc | null) => void;
  /**
   * Integração que o núcleo não ligou, como o real: `listar` a devolve `desconectado`;
   * `testar`, `conectar` e `desconectar` falham sempre com este erro (ERR-55). `null` liga.
   */
  desligar: (tipo: TipoIntegracao, erro: ErroIpc | null) => void;
  /** `true` se a integração está desligada (ver `desligar`). */
  desligada: (tipo: TipoIntegracao) => boolean;
};

/** Erro do catálogo no formato que o núcleo devolve num comando. */
export function erroDoCatalogo(
  codigo: ErroCatalogo["codigo"],
  id: string,
  parametros: Record<string, string> = {},
): ErroIpc {
  return { tipo: "catalogo", erro: { codigo, id, parametros } };
}

/** ERR-53 com o nome da integração e o motivo, como o núcleo real o monta. */
export function falhaInesperada(tipo: TipoIntegracao, motivo: string): ErroIpc {
  return erroDoCatalogo("INTEGRACAO_FALHA_INESPERADA", "ERR-53", {
    integracao: NOMES[tipo],
    motivo,
  });
}

/** ERR-55 com o motivo, como o núcleo real o devolve (ex.: integração não ligada). */
export function falhaInterna(motivo: string): ErroIpc {
  return erroDoCatalogo("FALHA_INTERNA", "ERR-55", { motivo });
}

export function criarNucleoSimuladoDeIntegracoes(
  inicial: Partial<Record<TipoIntegracao, Partial<Omit<Integracao, "tipo">>>> = {},
): NucleoSimuladoDeIntegracoes {
  const estados = new Map<TipoIntegracao, Integracao>(
    ORDEM.map((tipo) => [
      tipo,
      {
        tipo,
        status: "desconectado",
        conta: null,
        versao: null,
        notas_indexadas: null,
        erro: null,
        ...inicial[tipo],
      },
    ]),
  );
  const falhasDeChecagem = new Map<TipoIntegracao, ErroIpc>();
  const falhasDeConexao = new Map<TipoIntegracao, ErroIpc>();
  const falhasDeDesconexao = new Map<TipoIntegracao, ErroIpc>();
  const roteiros = new Map<TipoIntegracao, RoteiroDeConexao>();
  const pendentes = new Map<TipoIntegracao, Pendente>();
  const desligadas = new Map<TipoIntegracao, ErroIpc>();
  const exigirLigada = (tipo: TipoIntegracao) => {
    const erro = desligadas.get(tipo);
    if (erro) throw erro;
  };

  const estado = (tipo: TipoIntegracao): Integracao => ({ ...estados.get(tipo)! });

  const definir: NucleoSimuladoDeIntegracoes["definir"] = (tipo, parcial) => {
    estados.set(tipo, { ...estados.get(tipo)!, ...parcial, tipo });
  };

  const handlers: Partial<MockCommandHandlers> = {
    listarIntegracoes: () =>
      ORDEM.map((tipo) =>
        desligadas.has(tipo)
          ? { ...estado(tipo), status: "desconectado" as const, conta: null, erro: null }
          : estado(tipo),
      ),
    testarIntegracao: ({ tipo }) => {
      exigirLigada(tipo);
      const falha = falhasDeChecagem.get(tipo);
      if (falha) {
        falhasDeChecagem.delete(tipo);
        if (falha.tipo === "catalogo" && falha.erro.codigo === "INTEGRACAO_FALHA_INESPERADA") {
          definir(tipo, {
            status: "desconectado",
            conta: null,
            notas_indexadas: null,
            erro: falha.erro,
          });
        }
        throw falha;
      }
      return estado(tipo);
    },
    conectarIntegracao: ({ tipo, instrucoes }) => {
      exigirLigada(tipo);
      const roteiro = roteiros.get(tipo) ?? { instrucoes: [] };
      roteiros.delete(tipo);
      for (const instrucao of roteiro.instrucoes) instrucoes.onmessage(instrucao);
      const falha = falhasDeConexao.get(tipo);
      if (falha) {
        falhasDeConexao.delete(tipo);
        throw falha;
      }
      if (!roteiro.aguardarHumano) {
        definir(tipo, { status: "conectado", erro: null });
        return estado(tipo);
      }
      return new Promise<Integracao>((resolver, rejeitar) => {
        pendentes.set(tipo, { resolver, rejeitar });
      });
    },
    desconectarIntegracao: ({ tipo }) => {
      exigirLigada(tipo);
      const falha = falhasDeDesconexao.get(tipo);
      if (falha) {
        falhasDeDesconexao.delete(tipo);
        throw falha;
      }
      definir(tipo, { status: "desconectado", conta: null, notas_indexadas: null, erro: null });
      return estado(tipo);
    },
  };

  const pegarPendente = (tipo: TipoIntegracao): Pendente => {
    const p = pendentes.get(tipo);
    if (!p) throw new Error(`nenhuma conexão pendente de ${tipo} no núcleo simulado`);
    pendentes.delete(tipo);
    return p;
  };

  return {
    handlers,
    estado,
    definir,
    falharChecagem: (tipo, erro) => {
      if (erro) falhasDeChecagem.set(tipo, erro);
      else falhasDeChecagem.delete(tipo);
    },
    roteiroDeConexao: (tipo, roteiro) => roteiros.set(tipo, roteiro),
    falharConexaoDireto: (tipo, erro) => {
      if (erro) falhasDeConexao.set(tipo, erro);
      else falhasDeConexao.delete(tipo);
    },
    concluirConexao: (tipo, { conta, versao }) => {
      definir(tipo, {
        status: "conectado",
        erro: null,
        conta: conta ?? estados.get(tipo)!.conta,
        versao: versao ?? estados.get(tipo)!.versao,
      });
      pegarPendente(tipo).resolver(estado(tipo));
    },
    falharConexao: (tipo, erro) => pegarPendente(tipo).rejeitar(erro),
    falharDesconexao: (tipo, erro) => {
      if (erro) falhasDeDesconexao.set(tipo, erro);
      else falhasDeDesconexao.delete(tipo);
    },
    desligar: (tipo, erro) => {
      if (erro) desligadas.set(tipo, erro);
      else desligadas.delete(tipo);
    },
    desligada: (tipo) => desligadas.has(tipo),
  };
}
