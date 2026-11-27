//! Porta para quem acompanha uma execução do `claude -p` (plano B3 da onda 0).
//!
//! O executor do agente (E4-T4) lê a saída do processo com o
//! [`LeitorEventosCli`](super::evento_cli::LeitorEventosCli) e avisa cada observador, na
//! ordem das linhas: consumo (E6-T5, `RateLimit` e `Uso`), logs (E6-T6, ferramentas e
//! linhas ignoradas), chat (E4-T7, texto e resultado). O observador não abre processo nem
//! lê a saída: só reage ao que recebe.

use async_trait::async_trait;

use super::evento_cli::{EventoCli, LinhaIgnorada};

/// Como o processo do agente terminou, visto pelo executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FimExecucao {
    /// O processo saiu depois do `result`, com este código.
    Concluida {
        /// Código de saída do processo.
        codigo: i32,
    },
    /// O processo saiu sem mandar o `result` (caiu ou foi morto de fora).
    Morreu {
        /// Código de saída, quando o SO informa.
        codigo: Option<i32>,
    },
    /// O executor matou o processo depois do tempo sem nenhuma linha.
    Travou,
    /// O humano ou o núcleo cancelou a execução.
    Cancelada,
}

/// Quem acompanha uma execução. Todos os métodos têm corpo vazio: cada observador
/// implementa só o que usa. O executor chama na ordem das linhas e espera cada chamada.
#[async_trait]
pub trait ObservadorExecucao: Send + Sync {
    /// Um evento lido da saída da CLI.
    async fn ao_evento(&self, _evento: &EventoCli) {}

    /// Uma linha da saída que não virou evento (para o Log).
    async fn ao_ignorar_linha(&self, _motivo: &LinhaIgnorada) {}

    /// O processo terminou; é a última chamada da execução.
    async fn ao_fim(&self, _fim: FimExecucao) {}
}

/// Vários observadores, avisados um depois do outro, na ordem da lista.
#[async_trait]
impl<O: ObservadorExecucao> ObservadorExecucao for [O] {
    async fn ao_evento(&self, evento: &EventoCli) {
        for o in self {
            o.ao_evento(evento).await;
        }
    }

    async fn ao_ignorar_linha(&self, motivo: &LinhaIgnorada) {
        for o in self {
            o.ao_ignorar_linha(motivo).await;
        }
    }

    async fn ao_fim(&self, fim: FimExecucao) {
        for o in self {
            o.ao_fim(fim).await;
        }
    }
}

#[async_trait]
impl<O: ObservadorExecucao + ?Sized> ObservadorExecucao for std::sync::Arc<O> {
    async fn ao_evento(&self, evento: &EventoCli) {
        (**self).ao_evento(evento).await;
    }

    async fn ao_ignorar_linha(&self, motivo: &LinhaIgnorada) {
        (**self).ao_ignorar_linha(motivo).await;
    }

    async fn ao_fim(&self, fim: FimExecucao) {
        (**self).ao_fim(fim).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::domain::evento_cli::LeitorEventosCli;

    /// Guarda o que recebeu, com o nome do observador.
    struct Anotador {
        nome: &'static str,
        notas: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl ObservadorExecucao for Anotador {
        async fn ao_evento(&self, evento: &EventoCli) {
            let tipo = format!("{evento:?}");
            let tipo = tipo.split(['(', ' ']).next().unwrap_or_default().to_owned();
            self.notas
                .lock()
                .unwrap()
                .push(format!("{}:{tipo}", self.nome));
        }

        async fn ao_fim(&self, fim: FimExecucao) {
            self.notas
                .lock()
                .unwrap()
                .push(format!("{}:{fim:?}", self.nome));
        }
    }

    /// Só conta linhas ignoradas; os outros métodos ficam no corpo vazio.
    #[derive(Default)]
    struct Contador(Mutex<usize>);

    #[async_trait]
    impl ObservadorExecucao for Contador {
        async fn ao_ignorar_linha(&self, _motivo: &LinhaIgnorada) {
            *self.0.lock().unwrap() += 1;
        }
    }

    #[tokio::test]
    async fn observadores_recebem_na_ordem_das_linhas_e_da_lista() {
        let notas = Arc::new(Mutex::new(Vec::new()));
        let lista: Vec<Arc<dyn ObservadorExecucao>> = vec![
            Arc::new(Anotador {
                nome: "a",
                notas: notas.clone(),
            }),
            Arc::new(Anotador {
                nome: "b",
                notas: notas.clone(),
            }),
        ];
        let contador = Contador::default();
        let mut leitor = LeitorEventosCli::new();
        for linha in [
            r#"{"type":"system","subtype":"init","session_id":"s"}"#,
            "lixo",
            r#"{"type":"result","subtype":"success","is_error":false}"#,
        ] {
            match leitor.ler_linha(linha) {
                Ok(eventos) => {
                    for e in &eventos {
                        lista[..].ao_evento(e).await;
                        contador.ao_evento(e).await;
                    }
                }
                Err(motivo) => {
                    lista[..].ao_ignorar_linha(&motivo).await;
                    contador.ao_ignorar_linha(&motivo).await;
                }
            }
        }
        lista[..].ao_fim(FimExecucao::Concluida { codigo: 0 }).await;
        assert_eq!(
            *notas.lock().unwrap(),
            [
                "a:Init",
                "b:Init",
                "a:Resultado",
                "b:Resultado",
                "a:Concluida { codigo: 0 }",
                "b:Concluida { codigo: 0 }"
            ]
        );
        assert_eq!(*contador.0.lock().unwrap(), 1);
    }
}
