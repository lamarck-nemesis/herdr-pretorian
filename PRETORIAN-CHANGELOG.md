# Changelog — Herdr Pretorian

Mudanças relevantes do Herdr Pretorian, da mais recente para a mais antiga. O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e as versões seguem o [Versionamento Semântico](https://semver.org/lang/pt-BR/spec/v2.0.0.html).

- **Fonte da versão:** arquivo `VERSION` (independente da versão do Herdr upstream em `Cargo.toml`). Consulte com `pretorian --version`.
- **Como incrementar:** `MAJOR` para mudança incompatível (contrato, comando ou dado que exige ação de quem usa), `MINOR` para recurso novo compatível, `PATCH` para correção compatível. Enquanto a versão for `0.y.z`, mudanças incompatíveis sobem o `MINOR`.
- **No dia a dia:** cada PR com efeito para quem usa acrescenta uma linha em `[Não lançado]`, na categoria certa (Adicionado, Alterado, Obsoleto, Removido, Corrigido, Segurança).
- **Para lançar:** `python scripts/versao.py lancar patch|minor|major` move as entradas para a nova versão e atualiza `VERSION`; após o merge, crie a tag `pretorian-vX.Y.Z`. `python scripts/versao.py verificar` confere a consistência.

## [Não lançado]

## [0.7.0] - 2026-11-17

### Adicionado
- Painel de lembretes das análises locais mantidas pelo CLI 0.17.0, com leitura, cadastro manual e remoção do lembrete sem excluir a análise.
- Quatro arranjos da área de trabalho, barra de apoio e transferência da sessão viva para a divisão secundária, preservando os painéis ao mudar de direção.
- Configurações › usuário: lista das dependências da estação (conta Google no navegador, conferência do BB), lida do arquivo em `PRETORIAN_DEPENDENCIAS_ESTADO` mantido pelo lançador do CLI. Cada item mostra estado (pendente, bloqueado, em andamento, concluído, falhou), detalhe e ação; as setas passam do nome para a lista e Enter grava o pedido que o lançador atende abrindo a janela. Pendência acende o sinal de atenção no menu, em configurações e na aba usuário.

### Alterado
- Menu de raciocínio com Graduado, Especialista (padrão) e Doutor, migrando as preferências antigas para os três níveis. Requer o CLI 0.17.0 para aplicar os mesmos modelos e esforços.

## [0.6.2] - 2026-11-16

### Corrigido
- Abas: no painel gerenciado, a aba criada por atalho, sinal de mais ou menu de contexto passa `PRETORIAN_ABA_NOVA=1` ao lançador do CLI, que abre conversa nova em vez de retomar a de um painel fechado com o mesmo número.

## [0.6.1] - 2026-11-16

### Corrigido
- Perfil Pretorian do Windows Terminal e do Terminal do macOS (`just instalar`): a seleção com o mouse feita pelo próprio terminal ficava quase invisível (`#e0e4eb` sobre fundo branco). A cor passa a `#a9b1c9`, que se distingue do fundo e mantém o texto legível. Vale ao reinstalar o perfil.

## [0.6.0] - 2026-11-16

### Adicionado
- Integração com Claude e Codex: os hooks de estado do agente repassam o evento ao registrador de históricos do Pretorian CLI quando `PRETORIAN_HISTORICOS_PYTHON` e `PRETORIAN_HISTORICOS_HELPER` estão definidos, ligando o histórico nativo à conversa do Pretorian. A chamada tem limite de 0,5 s e falhas não afetam o agente. A versão das integrações sobe para Claude 12 e Codex 11.

## [0.5.1] - 2026-11-16

### Alterado
- Raciocínio: Graduado passa a ser o nível padrão quando não há preferência configurada; o menu e o exemplo de configuração acompanham esse padrão. Preferências explícitas continuam preservadas.

### Corrigido
- Referência de configuração: documenta `pretorian.pje_sem_janela` e mantém o padrão de raciocínio alinhado ao código.

## [0.5.0] - 2026-11-16

### Adicionado
- Menu: item **login pje** com o modo headless ligado ou desligado. A escolha fica em `pje_sem_janela`, na seção `[pretorian]` da configuração, e o Pretorian CLI a usa para fazer o login do PJe sem janela ou com a janela para acompanhar. O padrão é desligado (com janela).
- Janela do Pretorian: abrir uma aba mostra antes o diálogo "Nível técnico da nova aba" com os cinco níveis (teclas 1 a 5, setas e Enter, Esc cancela). A aba nasce com `PRETORIAN_NIVEL_ABA` no ambiente, que o lançador usa para ligar o agente no nível escolhido. Vale só quando o lançador informa `PRETORIAN_PAINEL_GERENCIADO=1`; fora da janela a aba nova abre como antes.

### Corrigido
- Menu: o item "raciocínio" passa a ser escrito em minúsculas, como os demais itens.
- Menu: o item de atualização do Pretorian fica sempre visível quando o lançador informa o arquivo de estado. Sem versão nova sinalizada ele aparece como "verificar atualização" e pede ao lançador que confira o catálogo; antes o item só existia depois que a conferência periódica encontrava versão nova.

## [0.4.0] - 2026-11-16

### Adicionado
- Menu Raciocínio com provedores, níveis de uso, preferência persistida e autorização oficial, incluindo envio manual do código Claude.
- Novas abas encaminhadas ao perfil gerenciado, com escolha compartilhada e preservação das conversas existentes.

## [0.3.0] - 2026-11-16

### Adicionado
- Diálogo de atualização com versão em uso, barra ASCII, andamento e resultado; cabeçalho acompanha a versão do Pretorian.

## [0.2.0] - 2026-11-16

### Adicionado
- Menu: item de atualização do Pretorian com estados ("atualizar o Pretorian", "atualizando o Pretorian…", "atualizado: vale ao reabrir", "atualização falhou: tentar de novo") e sinal de atenção no menu quando há versão nova. O estado vem do arquivo indicado por `PRETORIAN_ATUALIZACAO_ESTADO`, mantido pelo lançador do CLI; sem a variável o menu não muda.

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
