// TEL-01 Área de trabalho: uma grade com um espaço por componente lógico, na posição que o
// contrato dá a cada um (04-componentes.md). Cada espaço é da pasta da funcionalidade dona;
// esta tela só posiciona. TEL-02 e TEL-06 abrem por cima (NAV-03, NAV-15).
import { useState } from "react";
import { EspacoBarraDoChat } from "@/features/barra-do-chat";
import { EspacoPrompt, EspacoTerminal } from "@/features/chat";
import { BotaoConfiguracoes, TelaConfiguracoes } from "@/features/configuracoes";
import { EspacoConsumo } from "@/features/consumo";
import { EspacoContexto } from "@/features/contexto";
import { EspacoHistorico } from "@/features/historico";
import {
  IndicadorIntegracoes,
  PainelIntegracoes,
  ProvedorDeIntegracoes,
} from "@/features/integracoes";
import { EspacoLogs } from "@/features/logs";

// Colunas: esquerda (CMP-01, CMP-08), centro (CMP-02, CMP-04, CMP-06, CMP-07) e direita
// (CMP-03 com o botão Configurações, CMP-09).
const AREAS = `"consumo contexto integracoes" "logs terminal historico" "logs prompt historico" "logs barra historico"`;

export function MainWindow() {
  const [integracoesAbertas, setIntegracoesAbertas] = useState(false);
  const [configuracoesAbertas, setConfiguracoesAbertas] = useState(false);

  return (
    <ProvedorDeIntegracoes>
      <main
        className="grid h-screen grid-cols-[minmax(14rem,18rem)_1fr_minmax(16rem,22rem)] grid-rows-[minmax(7rem,auto)_1fr_auto_auto] gap-3 p-3"
        style={{ gridTemplateAreas: AREAS }}
      >
        <h1 className="sr-only">MyHarness</h1>
        <div style={{ gridArea: "consumo" }}>
          <EspacoConsumo />
        </div>
        <div style={{ gridArea: "contexto" }}>
          <EspacoContexto />
        </div>
        <div
          style={{ gridArea: "integracoes" }}
          data-cmp="CMP-03"
          className="flex flex-wrap items-start justify-end gap-2"
        >
          <IndicadorIntegracoes aoAbrir={() => setIntegracoesAbertas(true)} />
          <BotaoConfiguracoes aoAbrir={() => setConfiguracoesAbertas(true)} />
        </div>
        <div style={{ gridArea: "terminal" }} className="min-h-0">
          <EspacoTerminal />
        </div>
        <div style={{ gridArea: "prompt" }}>
          <EspacoPrompt />
        </div>
        <div style={{ gridArea: "barra" }}>
          <EspacoBarraDoChat />
        </div>
        <div style={{ gridArea: "logs" }} className="min-h-0">
          <EspacoLogs />
        </div>
        <div style={{ gridArea: "historico" }} className="min-h-0">
          <EspacoHistorico />
        </div>
      </main>
      <PainelIntegracoes aberto={integracoesAbertas} aoMudar={setIntegracoesAbertas} />
      <TelaConfiguracoes aberta={configuracoesAbertas} aoMudar={setConfiguracoesAbertas} />
    </ProvedorDeIntegracoes>
  );
}
