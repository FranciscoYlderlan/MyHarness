// Botão Configurações (TEL-01): abre a TEL-06 (NAV-15). Fica ao lado do indicador de
// integrações, no canto superior direito ([a confirmar, S15]).
import { Settings } from "lucide-react";
import { Button } from "@/components/ui/button";

export function BotaoConfiguracoes({ aoAbrir }: { aoAbrir: () => void }) {
  return (
    <Button variant="outline" size="sm" onClick={aoAbrir}>
      <Settings aria-hidden />
      Configurações
    </Button>
  );
}
