import { useState } from "react";
import {
  IndicadorIntegracoes,
  PainelIntegracoes,
  ProvedorDeIntegracoes,
} from "@/features/integracoes";

export function MainWindow() {
  const [integracoesAbertas, setIntegracoesAbertas] = useState(false);

  return (
    <ProvedorDeIntegracoes>
      <div className="flex min-h-screen flex-col">
        <header className="flex items-center justify-end p-3">
          <IndicadorIntegracoes aoAbrir={() => setIntegracoesAbertas(true)} />
        </header>
        <main className="flex flex-1 items-center justify-center">
          <h1 className="text-2xl font-semibold">MyHarness</h1>
        </main>
      </div>
      <PainelIntegracoes aberto={integracoesAbertas} aoMudar={setIntegracoesAbertas} />
    </ProvedorDeIntegracoes>
  );
}
