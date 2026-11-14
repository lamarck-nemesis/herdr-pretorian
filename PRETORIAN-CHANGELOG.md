# Changelog — Herdr Pretorian

Mudanças relevantes do Herdr Pretorian, da mais recente para a mais antiga. O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e as versões seguem o [Versionamento Semântico](https://semver.org/lang/pt-BR/spec/v2.0.0.html).

- **Fonte da versão:** arquivo `VERSION` (independente da versão do Herdr upstream em `Cargo.toml`). Consulte com `pretorian --version`.
- **Como incrementar:** `MAJOR` para mudança incompatível (contrato, comando ou dado que exige ação de quem usa), `MINOR` para recurso novo compatível, `PATCH` para correção compatível. Enquanto a versão for `0.y.z`, mudanças incompatíveis sobem o `MINOR`.
- **No dia a dia:** cada PR com efeito para quem usa acrescenta uma linha em `[Não lançado]`, na categoria certa (Adicionado, Alterado, Obsoleto, Removido, Corrigido, Segurança).
- **Para lançar:** `python scripts/versao.py lancar patch|minor|major` move as entradas para a nova versão e atualiza `VERSION`; após o merge, crie a tag `pretorian-vX.Y.Z`. `python scripts/versao.py verificar` confere a consistência.

## [Não lançado]

## [0.1.2] - 2026-11-14

### Alterado
- Tema Pretorian no terminal inteiro, marca no título da janela, nome na barra lateral e menu simplificado.
- Instalação por `just instalar` configura o perfil Pretorian no terminal do Windows e do macOS.

## [0.1.1] - 2026-11-14

### Corrigido
- Build macOS usa o runner macOS 15 e verifica a execução nativa do binário Apple Silicon.

### Alterado
- Reconstrução dos aplicativos da linha v2 com identificação Pretorian 0.1.1, mantendo a base Herdr 0.9.5.

## [0.1.0] - 2026-11-14

Primeira versão numerada do fork, sobre o Herdr 0.9.5.

### Adicionado
- Versionamento semântico próprio: `pretorian --version` mostra a versão do Pretorian e a do Herdr de base; tags `pretorian-vX.Y.Z`.
- Tema claro `pretorian`, título externo `Pretorian` e cabeçalho `Pretorian · Nome` passam a ser o padrão embutido; um `config.toml` explícito continua prevalecendo.
- Nova seção **usuário** nas configurações grava `[pretorian] nome` e atualiza o cabeçalho sem reiniciar.
- Configuração e estado ficam em `~/.config/pretorian` e `~/.local/state/pretorian`, sem tocar em um Herdr pessoal.
- Botões `esc fechar` e `↵ continuar` deixam de sair cortados; avisos acentuados são medidos em colunas.
- Mensagens próprias de conexão, sessão, servidor, ponte remota e validação de configuração traduzidas; `just instalar` compila e instala `pretorian` a partir do clone.
- `just instalar` no Windows grava `%LOCALAPPDATA%\Programs\pretorian\pretorian.exe` com a pasta `conpty` ao lado; `NOTICE` registra que este é um fork modificado do Herdr.
- No tema `pretorian` os painéis usam fundo e texto próprios (`#ffffff`/`#2f3344`) e informam tema claro aos programas internos, sem depender das cores do terminal hospedeiro; foot e Windows Terminal via WSL deixam de mostrar painéis escuros ao lado da barra lateral clara.
