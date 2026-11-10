//! Cofre de credenciais do sistema (RN-04, RNF-05, D12).
//!
//! O token do GitHub (ENT-13 `credencial`) mora só aqui: no Windows, o Gerenciador de
//! Credenciais ([`CofreDoSistema`], pela crate `keyring`); nos testes, a memória
//! ([`CofreEmMemoria`]). Nada deste módulo grava em arquivo, SQLite, log ou cofre
//! Obsidian, e nenhum tipo mostra o valor guardado em `Debug` ou `Display`: o valor sai
//! só como [`Segredo`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use my_harness_core::domain::segredo::{Segredo, TextoMascarado};

/// Serviço sob o qual o MyHarness guarda as credenciais (identificador do app, D13).
pub const SERVICO: &str = "dev.myharness.app";

/// Onde uma credencial mora no cofre: serviço e conta.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChaveCredencial {
    /// Serviço (no Windows, parte do nome do alvo da credencial).
    pub servico: String,
    /// Conta dentro do serviço.
    pub conta: String,
}

impl ChaveCredencial {
    /// Chave com serviço e conta dados.
    pub fn nova(servico: impl Into<String>, conta: impl Into<String>) -> Self {
        Self {
            servico: servico.into(),
            conta: conta.into(),
        }
    }

    /// Chave do token do device flow do GitHub (RF-03).
    pub fn github() -> Self {
        Self::nova(SERVICO, "github-oauth-token")
    }
}

/// Falha do cofre de credenciais. O motivo vem mascarado (RN-04) e nunca traz o valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroCofre {
    motivo: TextoMascarado,
}

impl ErroCofre {
    /// Erro com o motivo dado (mascarado na construção).
    pub fn novo(motivo: &str) -> Self {
        Self {
            motivo: TextoMascarado::new(motivo),
        }
    }

    /// O motivo, mascarado.
    pub fn motivo(&self) -> &TextoMascarado {
        &self.motivo
    }
}

impl fmt::Display for ErroCofre {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cofre de credenciais do sistema: {}", self.motivo)
    }
}

impl std::error::Error for ErroCofre {}

/// Cofre de credenciais do sistema. As chamadas são curtas e síncronas.
pub trait CofreCredenciais: Send + Sync + fmt::Debug {
    /// Valor guardado em `chave`; `None` se não há credencial.
    fn ler(&self, chave: &ChaveCredencial) -> Result<Option<Segredo>, ErroCofre>;
    /// Guarda `valor` em `chave`, substituindo o anterior.
    fn gravar(&self, chave: &ChaveCredencial, valor: &Segredo) -> Result<(), ErroCofre>;
    /// Apaga a credencial de `chave`; sem credencial, não faz nada.
    fn apagar(&self, chave: &ChaveCredencial) -> Result<(), ErroCofre>;
}

/// Cofre do sistema operacional: Gerenciador de Credenciais no Windows (D12); Keychain
/// no macOS e Secret Service no Linux, só para desenvolvimento.
#[derive(Debug, Default, Clone, Copy)]
pub struct CofreDoSistema;

impl CofreDoSistema {
    fn entrada(chave: &ChaveCredencial) -> Result<keyring::Entry, ErroCofre> {
        keyring::Entry::new(&chave.servico, &chave.conta).map_err(|e| erro_do_sistema(&e))
    }
}

/// Traduz o erro do `keyring` pelo `Display`, que não traz o valor guardado.
fn erro_do_sistema(erro: &keyring::Error) -> ErroCofre {
    ErroCofre::novo(&erro.to_string())
}

impl CofreCredenciais for CofreDoSistema {
    fn ler(&self, chave: &ChaveCredencial) -> Result<Option<Segredo>, ErroCofre> {
        match Self::entrada(chave)?.get_password() {
            Ok(valor) => Ok(Some(Segredo::new(valor))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(erro_do_sistema(&e)),
        }
    }

    fn gravar(&self, chave: &ChaveCredencial, valor: &Segredo) -> Result<(), ErroCofre> {
        Self::entrada(chave)?
            .set_password(valor.expor())
            .map_err(|e| erro_do_sistema(&e))
    }

    fn apagar(&self, chave: &ChaveCredencial) -> Result<(), ErroCofre> {
        match Self::entrada(chave)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(erro_do_sistema(&e)),
        }
    }
}

/// Cofre em memória, para os testes automáticos. [`CofreEmMemoria::quebrar`] faz as
/// próximas chamadas falharem, para testar o erro do cofre.
#[derive(Debug, Default)]
pub struct CofreEmMemoria {
    valores: Mutex<HashMap<ChaveCredencial, Segredo>>,
    falha: Mutex<Option<String>>,
}

impl CofreEmMemoria {
    /// Cofre vazio.
    pub fn novo() -> Self {
        Self::default()
    }

    /// Faz toda chamada seguinte falhar com `motivo` (`None` volta ao normal).
    pub fn quebrar(&self, motivo: Option<&str>) {
        *self.falha.lock().expect("cofre em memória") = motivo.map(str::to_string);
    }

    fn falhou(&self) -> Result<(), ErroCofre> {
        match &*self.falha.lock().expect("cofre em memória") {
            Some(motivo) => Err(ErroCofre::novo(motivo)),
            None => Ok(()),
        }
    }
}

impl CofreCredenciais for CofreEmMemoria {
    fn ler(&self, chave: &ChaveCredencial) -> Result<Option<Segredo>, ErroCofre> {
        self.falhou()?;
        Ok(self
            .valores
            .lock()
            .expect("cofre em memória")
            .get(chave)
            .cloned())
    }

    fn gravar(&self, chave: &ChaveCredencial, valor: &Segredo) -> Result<(), ErroCofre> {
        self.falhou()?;
        self.valores
            .lock()
            .expect("cofre em memória")
            .insert(chave.clone(), valor.clone());
        Ok(())
    }

    fn apagar(&self, chave: &ChaveCredencial) -> Result<(), ErroCofre> {
        self.falhou()?;
        self.valores.lock().expect("cofre em memória").remove(chave);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALOR: &str = "valor-secreto-sem-prefixo-9f8e7d";

    #[test]
    fn grava_le_e_apaga() {
        let cofre = CofreEmMemoria::novo();
        let chave = ChaveCredencial::github();
        assert_eq!(cofre.ler(&chave).unwrap(), None);

        cofre.gravar(&chave, &Segredo::new(VALOR)).unwrap();
        assert_eq!(cofre.ler(&chave).unwrap().unwrap().expor(), VALOR);

        cofre.apagar(&chave).unwrap();
        assert_eq!(cofre.ler(&chave).unwrap(), None);
        // Apagar sem credencial não é erro.
        cofre.apagar(&chave).unwrap();
    }

    #[test]
    fn debug_do_cofre_nao_mostra_o_valor() {
        let cofre = CofreEmMemoria::novo();
        cofre
            .gravar(&ChaveCredencial::github(), &Segredo::new(VALOR))
            .unwrap();
        let saida = format!("{cofre:?} {cofre:#?}");
        assert!(!saida.contains(VALOR), "{saida}");
        assert!(saida.contains("••••"), "{saida}");
    }

    #[test]
    fn falha_do_cofre_vem_mascarada() {
        let cofre = CofreEmMemoria::novo();
        cofre.quebrar(Some("acesso negado a gho_A1b2C3d4E5f6G7h8"));
        let erro = cofre.ler(&ChaveCredencial::github()).unwrap_err();
        let saida = format!("{erro} {erro:?}");
        assert!(!saida.contains("A1b2C3d4"), "{saida}");
        assert_eq!(
            erro.to_string(),
            "cofre de credenciais do sistema: acesso negado a ••••"
        );
        cofre.quebrar(None);
        assert_eq!(cofre.ler(&ChaveCredencial::github()).unwrap(), None);
    }
}
