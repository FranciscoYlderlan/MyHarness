// Liga a tela das integrações ao núcleo, só pelo contrato IPC (`@/lib/ipc`): lista as três
// ao abrir, ouve `integracao-mudou` e roda conectar e desconectar com o canal de
// instruções. Toda resposta de comando passa pelo zod; toda falha vira um item do catálogo
// no cartão da integração.
import { useCallback, useEffect, useMemo, useReducer, useRef, type ReactNode } from "react";
import { z } from "zod";
import { canalDeConexao, commands, ouvir, validarResposta } from "@/lib/ipc";
import type { Integracao, TipoIntegracao } from "@/lib/ipc";
import { integracao as esquemaIntegracao } from "@/lib/ipc/esquemas";
import { ContextoDasIntegracoes, type Integracoes } from "./contexto";
import { estadoInicial, reduzir, type Operacao } from "./estado";
import { erroDoComando, ORDEM_DAS_INTEGRACOES } from "./mensagens";

type Resultado = { status: "ok"; data: Integracao } | { status: "error"; error: unknown };

const listaDeIntegracoes = z.array(esquemaIntegracao);

export function ProvedorDeIntegracoes({ children }: { children: ReactNode }) {
  const [estado, despachar] = useReducer(reduzir, undefined, estadoInicial);
  const estadoRef = useRef(estado);
  useEffect(() => {
    estadoRef.current = estado;
  }, [estado]);
  const montado = useRef(true);
  // Só a resposta da última operação de cada integração vale (outra pode ter começado).
  const vez = useRef<Record<TipoIntegracao, number>>({
    claude_cli: 0,
    obsidian_cli: 0,
    github: 0,
  });

  // Guarda de operação em andamento: uma operação por integração de cada vez (checar não
  // passa por cima de desconectar, nem dois conectar ao mesmo tempo). Ref, e não o estado,
  // para valer já no mesmo instante, antes do React redesenhar.
  const andamento = useRef<Record<TipoIntegracao, Operacao | null>>({
    claude_cli: null,
    obsidian_cli: null,
    github: null,
  });

  const rodar = useCallback(
    async (tipo: TipoIntegracao, operacao: Operacao, chamada: () => Promise<Resultado>) => {
      if (andamento.current[tipo] !== null) return;
      andamento.current[tipo] = operacao;
      const minha = ++vez.current[tipo];
      despachar({ tipo: "iniciou", integracao: tipo, operacao });
      let resultado: Resultado;
      try {
        resultado = validarResposta(`resposta de ${operacao}`, esquemaIntegracao, await chamada());
      } catch (e) {
        resultado = { status: "error", error: e };
      } finally {
        andamento.current[tipo] = null;
      }
      if (!montado.current || vez.current[tipo] !== minha) return;
      if (resultado.status === "ok") despachar({ tipo: "recebeu", integracao: resultado.data });
      else
        despachar({
          tipo: "falhou",
          integracao: tipo,
          operacao,
          erro: erroDoComando(resultado.error),
        });
    },
    [],
  );

  const checar = useCallback(
    (tipo: TipoIntegracao) => {
      void rodar(tipo, "checar", () => commands.testarIntegracao(tipo));
    },
    [rodar],
  );

  const checarTodas = useCallback(() => {
    for (const tipo of ORDEM_DAS_INTEGRACOES) checar(tipo);
  }, [checar]);

  const conectar = useCallback(
    (tipo: TipoIntegracao) => {
      void rodar(tipo, "conectar", () => {
        const minha = vez.current[tipo];
        const canal = canalDeConexao((instrucao) => {
          if (montado.current && vez.current[tipo] === minha)
            despachar({ tipo: "instrucao", integracao: tipo, instrucao });
        });
        return commands.conectarIntegracao(tipo, canal);
      });
    },
    [rodar],
  );

  const desconectar = useCallback(
    (tipo: TipoIntegracao) => {
      void rodar(tipo, "desconectar", () => commands.desconectarIntegracao(tipo));
    },
    [rodar],
  );

  // O erro que já foi repetido: duplo clique em "Repetir" com o núcleo instantâneo chega
  // depois da resposta e antes do React redesenhar, com o mesmo erro ainda na tela.
  const repetido = useRef<Record<TipoIntegracao, unknown>>({
    claude_cli: null,
    obsidian_cli: null,
    github: null,
  });

  const repetir = useCallback(
    (tipo: TipoIntegracao) => {
      if (andamento.current[tipo] !== null) return;
      const erro = estadoRef.current[tipo].erro;
      if (erro !== null && repetido.current[tipo] === erro) return;
      repetido.current[tipo] = erro;
      const operacao = erro?.operacao ?? "checar";
      if (operacao === "conectar") conectar(tipo);
      else if (operacao === "desconectar") desconectar(tipo);
      else checar(tipo);
    },
    [checar, conectar, desconectar],
  );

  // Abertura do app: lista as três e checa cada uma (UC-01, gatilho "o app abre").
  useEffect(() => {
    montado.current = true;
    let pararDeOuvir: (() => void) | null = null;

    ouvir("integracaoMudou", ({ integracao }) => {
      if (montado.current) despachar({ tipo: "mudou", integracao });
    })
      .then((parar) => {
        if (montado.current) pararDeOuvir = parar;
        else parar();
      })
      .catch(() => {
        // Sem núcleo (tela fora do app): os comandos abaixo mostram a falha nos cartões.
      });

    // `listar` já checa as três e traz o erro de cada uma (M-46); só se ele falhar a
    // tela checa uma a uma, para cada cartão mostrar a falha. O resultado respeita a guarda:
    // a integração com operação iniciada depois do pedido fica com a resposta dela (o
    // `listar` não apaga o código do device flow de um `conectar` em andamento).
    const vezNoPedido = { ...vez.current };
    void (async () => {
      try {
        const lista = validarResposta(
          "resposta de listar",
          listaDeIntegracoes,
          await commands.listarIntegracoes(),
        );
        if (!montado.current) return;
        if (lista.status === "ok") {
          for (const integracao of lista.data) {
            const tipo = integracao.tipo;
            if (andamento.current[tipo] !== null || vez.current[tipo] !== vezNoPedido[tipo])
              continue;
            despachar({ tipo: "mudou", integracao });
          }
          return;
        }
      } catch {
        // Sem núcleo: a checagem de cada integração, logo abaixo, mostra o erro.
      }
      if (montado.current) checarTodas();
    })();

    return () => {
      montado.current = false;
      try {
        // Sem o núcleo (ex.: depois de fechado), parar de ouvir pode falhar: nada a fazer.
        void Promise.resolve(pararDeOuvir?.()).catch(() => {});
      } catch {
        // idem
      }
    };
  }, [checarTodas]);

  const valor = useMemo<Integracoes>(
    () => ({ estado, checar, checarTodas, conectar, desconectar, repetir }),
    [estado, checar, checarTodas, conectar, desconectar, repetir],
  );

  return (
    <ContextoDasIntegracoes.Provider value={valor}>{children}</ContextoDasIntegracoes.Provider>
  );
}
