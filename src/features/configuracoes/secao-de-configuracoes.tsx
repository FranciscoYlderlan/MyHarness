// Uma seção da TEL-06 Configurações: título, uma frase do que ela ajusta e, por enquanto,
// o espaço reservado do conteúdo (ID do contrato).
import { MarcadorDeEspaco } from "@/components/marcador-de-espaco";

export function SecaoDeConfiguracoes({
  id,
  titulo,
  descricao,
}: {
  /** ID do componente ou da entidade no contrato, ex.: "CMP-16". */
  id: string;
  titulo: string;
  descricao: string;
}) {
  const idDoTitulo = `secao-${id.toLowerCase()}`;
  return (
    <section aria-labelledby={idDoTitulo} data-secao={id} className="space-y-2">
      <h3 id={idDoTitulo} className="text-base font-medium">
        {titulo}
      </h3>
      <p className="text-sm text-muted-foreground">{descricao}</p>
      <MarcadorDeEspaco cmp={id} nome={titulo} />
    </section>
  );
}
