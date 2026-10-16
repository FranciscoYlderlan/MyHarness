// Liga a tela das integrações ao núcleo, só pelo contrato IPC (`@/lib/ipc`): lista e checa
// as três ao abrir, ouve `integracao-mudou` e roda conectar e desconectar com o canal de
// instruções. Toda falha vira um item do catálogo no cartão da integração.
import { useCallback, useEffect, useMemo, useReducer, useRef, type ReactNode } from "react";
import { canalDeConexao, commands, ouvir } from "@/lib/ipc";
import type { Integracao, TipoIntegracao } from "@/lib/ipc";
import { ContextoDasIntegracoes, type Integracoes } from "./contexto";
import { estadoInicial, reduzir, type Operacao } from "./estado";
import { erroDoComando, ORDEM_DAS_INTEGRACOES } from "./mensagens";

type Resultado = { status: "ok"; data: Integracao } | { status: "error"; error: unknown };

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

  const rodar = useCallback(
    async (tipo: TipoIntegracao, operacao: Operacao, chamada: () => Promise<Resultado>) => {
      const minha = ++vez.current[tipo];
      despachar({ tipo: "iniciou", integracao: tipo, operacao });
      let resultado: Resultado;
      try {
        resultado = await chamada();
      } catch (e) {
        resultado = { status: "error", error: e };
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
      if (estadoRef.current[tipo].emAndamento === "conectar") return;
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

  const repetir = useCallback(
    (tipo: TipoIntegracao) => {
      const operacao = estadoRef.current[tipo].erro?.operacao ?? "checar";
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

    void (async () => {
      try {
        const lista = await commands.listarIntegracoes();
        if (!montado.current) return;
        if (lista.status === "ok") {
          for (const integracao of lista.data) despachar({ tipo: "recebeu", integracao });
        }
      } catch {
        // A checagem de cada integração, logo abaixo, mostra o erro.
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
