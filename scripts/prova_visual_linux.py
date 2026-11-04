import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import subprocess
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'tests/fixtures/pretorian-visual'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def ler_candidato(pasta, commit, zip_sha, bin_sha):
    registro = json.loads((pasta / 'candidato.json').read_text())
    if (registro.get('estado') != 'candidato' or registro.get('repositorio') != 'lamarck-nemesis/herdr-pretorian'
            or registro.get('fonte_commit') != commit or registro.get('versao') != 'candidato-' + commit[:12]):
        raise ValueError('Origem do candidato divergente')
    item = registro['artefatos']['linux-x64']
    nome = f'pretorian-fork-linux-x64-{commit[:12]}.zip'
    if item['arquivo'] != nome or item['sha256'] != zip_sha or item['arquivos']['herdr-pretorian'] != bin_sha:
        raise ValueError('Manifesto do candidato divergente')
    bruto = (pasta / nome).read_bytes()
    if digest(bruto) != zip_sha:
        raise ValueError('Hash do ZIP divergente')
    with zipfile.ZipFile(pasta / nome) as arquivo:
        membros = arquivo.infolist()
        nomes = [m.filename for m in membros]
        if len(nomes) != len(set(nomes)) or set(nomes) != set(item['arquivos']):
            raise ValueError('Conteúdo do ZIP divergente')
        if len(membros) > 256 or sum(m.file_size for m in membros) > 256 * 1024 * 1024:
            raise ValueError('ZIP excede os limites')
        for membro in membros:
            if digest(arquivo.read(membro)) != item['arquivos'][membro.filename]:
                raise ValueError('Arquivo do ZIP divergente')
        binario = arquivo.read('herdr-pretorian')
    if digest(binario) != bin_sha:
        raise ValueError('Hash do executável divergente')
    return binario


def parar(processo):
    if processo is not None:
        try:
            processo.terminate()
        except ProcessLookupError:
            pass
        try:
            processo.wait(timeout=5)
        except subprocess.TimeoutExpired:
            processo.kill()
            processo.wait(timeout=5)


def processos_candidato(exe):
    encontrados = []
    for entrada in Path('/proc').iterdir():
        if not entrada.name.isdigit():
            continue
        try:
            if os.path.samefile(entrada / 'exe', exe):
                encontrados.append(int(entrada.name))
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass
    return encontrados


def candidato_no_terminal(exe, terminal_pid):
    for pid in processos_candidato(exe):
        for _ in range(64):
            if pid == terminal_pid:
                return True
            try:
                campos = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
                pid = int(campos[1])
            except (FileNotFoundError, PermissionError, IndexError, ValueError):
                break
            if pid <= 1:
                break
    return False


def aguardar_gerenciador(ambiente, wm, timeout=10):
    prazo = time.monotonic() + timeout
    while True:
        restante = prazo - time.monotonic()
        if restante <= 0:
            break
        if wm.poll() is not None:
            raise RuntimeError('Gerenciador de janelas encerrou antes de ficar pronto')
        raiz = subprocess.run(['xprop', '-root', '_NET_SUPPORTING_WM_CHECK', '_NET_SUPPORTED'],
                              env=ambiente, capture_output=True, text=True, timeout=min(2, restante))
        identificador = re.search(r'_NET_SUPPORTING_WM_CHECK\(WINDOW\):\s*window id # (0x[0-9a-fA-F]+)', raiz.stdout)
        suporte = re.search(r'(?<![\w])_NET_ACTIVE_WINDOW(?![\w])', raiz.stdout)
        if raiz.returncode == 0 and identificador and int(identificador.group(1), 16) and suporte:
            restante = prazo - time.monotonic()
            if restante <= 0:
                break
            janela = identificador.group(1)
            filho = subprocess.run(['xprop', '-id', janela, '_NET_SUPPORTING_WM_CHECK', '_NET_WM_NAME'],
                                   env=ambiente, capture_output=True, text=True, timeout=min(2, restante))
            confirmacao = re.search(r'_NET_SUPPORTING_WM_CHECK\(WINDOW\):\s*window id # (0x[0-9a-fA-F]+)', filho.stdout)
            nome = re.search(r'_NET_WM_NAME\(UTF8_STRING\)\s*=\s*"Openbox"', filho.stdout, re.IGNORECASE)
            if filho.returncode == 0 and confirmacao and int(confirmacao.group(1), 16) == int(janela, 16) and nome:
                if time.monotonic() >= prazo:
                    break
                if wm.poll() is not None:
                    raise RuntimeError('Gerenciador de janelas encerrou antes de ficar pronto')
                return
        restante = prazo - time.monotonic()
        if restante <= 0:
            break
        time.sleep(min(0.1, restante))
    raise TimeoutError('Gerenciador de janelas não confirmou suporte à ativação')


def stderr_sanitizado(arquivo):
    arquivo.flush()
    arquivo.seek(0)
    conteudo = arquivo.read(16385)
    texto = conteudo[:16384].decode('utf-8', errors='replace')
    texto = ''.join(c for c in texto if c.isprintable() or c in '\n\t')
    texto = re.sub(r'(?im)\b(token|password|senha|authorization|secret)[ \t]*[:=][^\n]*', r'\1=[removido]', texto)
    texto = re.sub(r'https?://\S+', '[endereço removido]', texto)
    texto = re.sub(r'(?<![\w])/(?:[^\s:\'\"]+)', '[caminho removido]', texto)
    return {'texto': texto, 'truncado': len(conteudo) > 16384}


def estado_mapa(texto):
    estado = re.search(r'Map State:\s*(IsViewable|IsUnviewable|IsUn[Mm]apped)\b', texto)
    if not estado:
        return 'desconhecido'
    return 'IsUnmapped' if estado.group(1) in ('IsUnMapped', 'IsUnmapped') else estado.group(1)


def inventario_janelas(ambiente, terminal_pid):
    try:
        busca = subprocess.run(['xdotool', 'search', '--class', 'XTerm'], env=ambiente,
                               capture_output=True, text=True, timeout=5)
        arvore = subprocess.run(['xwininfo', '-root', '-tree'], env=ambiente,
                                capture_output=True, text=True, timeout=5)
        ids = {int(s) for s in busca.stdout.splitlines() if s.isascii() and s.isdigit()}
        ids.update(int(s, 16) for s in re.findall(r'^\s*(0x[0-9a-fA-F]+)\b', arvore.stdout, re.MULTILINE))
        janelas = []
        for numero in sorted(ids)[:64]:
            janela = str(numero)
            props = subprocess.run(['xprop', '-id', janela, '_NET_WM_PID', 'WM_CLASS', 'WM_NAME', 'WM_STATE'],
                                   env=ambiente, capture_output=True, text=True, timeout=5)
            info = subprocess.run(['xwininfo', '-id', janela], env=ambiente,
                                  capture_output=True, text=True, timeout=5)
            pid = re.search(r'_NET_WM_PID\(CARDINAL\)\s*=\s*(\d+)', props.stdout)
            wm_state = re.search(r'window state:\s*(Normal|Iconic|Withdrawn)\b', props.stdout)
            pai_estado = None
            corresponde = bool(pid and int(pid.group(1)) == terminal_pid)
            if corresponde:
                arvore_pai = subprocess.run(['xwininfo', '-id', janela, '-tree'], env=ambiente,
                                            capture_output=True, text=True, timeout=5)
                pai = re.search(r'Parent window id:\s*(0x[0-9a-fA-F]+)', arvore_pai.stdout)
                if pai:
                    info_pai = subprocess.run(['xwininfo', '-id', pai.group(1)], env=ambiente,
                                              capture_output=True, text=True, timeout=5)
                    pai_estado = estado_mapa(info_pai.stdout)
                else:
                    pai_estado = 'desconhecido'
            janelas.append({'id': janela, 'pid': int(pid.group(1)) if pid else None,
                            'pid_corresponde_terminal': corresponde,
                            'classe_xterm': '"XTerm"' in props.stdout,
                            'titulo_pretorian': '"Pretorian"' in props.stdout,
                            'estado': estado_mapa(info.stdout), 'estado_pai': pai_estado,
                            'estado_wm': wm_state.group(1) if wm_state else 'desconhecido',
                            'xprop_status': props.returncode, 'xwininfo_status': info.returncode})
        return {'busca_status': busca.returncode, 'arvore_status': arvore.returncode,
                'quantidade_total': len(ids), 'janelas': janelas}
    except (OSError, subprocess.SubprocessError) as erro:
        return {'erro_tipo': type(erro).__name__, 'janelas': []}


def processos_marcados(marcador):
    chave = ('PRETORIAN_PROVA_VISUAL=' + marcador).encode()
    encontrados = []
    for entrada in Path('/proc').iterdir():
        if not entrada.name.isdigit():
            continue
        try:
            campos = (entrada / 'stat').read_text().rsplit(')', 1)[1].split()
            if campos[0] != 'Z' and chave in (entrada / 'environ').read_bytes().split(b'\0'):
                encontrados.append((int(entrada.name), campos[19]))
        except (FileNotFoundError, ProcessLookupError, PermissionError, IndexError):
            continue
    return encontrados


def limpar_processos(marcador):
    prazo = time.monotonic() + 5
    while time.monotonic() < prazo:
        encontrados = processos_marcados(marcador)
        if not encontrados:
            return
        for pid, inicio in encontrados:
            try:
                descritor = os.pidfd_open(pid)
            except ProcessLookupError:
                continue
            try:
                campos = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
                ambiente = Path(f'/proc/{pid}/environ').read_bytes().split(b'\0')
                if campos[19] == inicio and ('PRETORIAN_PROVA_VISUAL=' + marcador).encode() in ambiente:
                    signal.pidfd_send_signal(descritor, signal.SIGKILL)
            except (FileNotFoundError, ProcessLookupError):
                pass
            finally:
                os.close(descritor)
        time.sleep(0.05)
    if processos_marcados(marcador):
        raise RuntimeError('Processos da prova permaneceram ativos')


@contextmanager
def ambiente_temporario(marcador):
    casa = Path(tempfile.mkdtemp(prefix='visual-', dir=os.environ['RUNNER_TEMP']))
    try:
        yield casa
    finally:
        limpar_processos(marcador)
        shutil.rmtree(casa)


def main():
    import tomllib

    parser = argparse.ArgumentParser()
    for nome in ('candidato', 'fonte', 'destino', 'commit', 'zip-sha', 'bin-sha', 'run', 'tentativa', 'artefato'):
        parser.add_argument('--' + nome, required=True)
    args = parser.parse_args()
    for valor, padrao in ((args.commit, r'[a-f0-9]{40}'), (args.zip_sha, r'[a-f0-9]{64}'),
                           (args.bin_sha, r'[a-f0-9]{64}'), (args.run, r'[0-9]+'), (args.tentativa, r'[0-9]+'),
                           (args.artefato, r'[0-9]+')):
        if not re.fullmatch(padrao, valor):
            raise ValueError('Identificação inválida')
    origem = json.loads((FIXTURE / 'origem.json').read_text())
    for nome, esperado in origem['arquivos'].items():
        if digest((FIXTURE / nome).read_bytes()) != esperado:
            raise ValueError('Dados de aparência alterados')
    config = (FIXTURE / 'aparencia.toml').read_bytes()
    dados = tomllib.loads(config.decode())
    if set(dados) != {'terminal', 'ui', 'theme'} or set(dados['ui']) != {'window_title', 'tab_bar_right'}:
        raise ValueError('Configuração de aparência inesperada')
    tema = dados['theme']
    campos = {'accent', 'panel_bg', 'sidebar_bg', 'active_row_bg', 'selection_bg', 'surface0', 'surface1',
              'surface_dim', 'overlay0', 'overlay1', 'text', 'subtext0', 'mauve', 'green', 'yellow',
              'red', 'blue', 'teal', 'peach'}
    if (dados['terminal'] != {'new_cwd': 'current'} or set(tema) != {'name', 'auto_switch', 'custom'}
            or tema['name'] != 'catppuccin-latte' or tema['auto_switch'] is not False
            or set(tema['custom']) != campos
            or not all(isinstance(valor, str) and re.fullmatch(r'#[a-f0-9]{6}', valor)
                       for valor in tema['custom'].values())):
        raise ValueError('Dados não permitidos na aparência')
    if dados['ui']['window_title'] != 'Pretorian' or dados['ui']['tab_bar_right'] != [{'type': 'text', 'text': 'Pretorian'}]:
        raise ValueError('Título divergente')
    args_xterm = json.loads((FIXTURE / 'xterm.json').read_text())
    if args_xterm != ['@XTERM@', '-fa', 'JetBrains Mono', '-fs', '13', '-T', 'Pretorian', '-bg', '#f6f7f9', '-fg', '#2f3344', '-e', '@DESTINO@/pretorian', '--interno']:
        raise ValueError('Argumentos de aparência divergentes')
    fonte = Path(args.fonte).read_bytes()
    if digest(fonte) != origem['fonte_sha256']:
        raise ValueError('Fonte divergente')
    binario = ler_candidato(Path(args.candidato), args.commit, args.zip_sha, args.bin_sha)
    destino = Path(args.destino).resolve()
    destino.mkdir(parents=True, exist_ok=True)
    ambiente = {k: os.environ[k] for k in ('DISPLAY', 'XAUTHORITY') if k in os.environ}
    if 'DISPLAY' not in ambiente:
        raise ValueError('Display X11 ausente')
    wm = terminal = None
    marcador = secrets.token_hex(24)
    with ambiente_temporario(marcador) as casa:
        for pasta in ('data/fonts/Pretorian', 'config', 'cache', 'estado', 'socket', 'trabalho'):
            (casa / pasta).mkdir(parents=True, exist_ok=True)
        (casa / 'socket').chmod(0o700)
        fonte_path = casa / 'data/fonts/Pretorian/JetBrainsMono-Regular.ttf'
        fonte_path.write_bytes(fonte)
        config_path = casa / 'config.toml'
        config_path.write_bytes(config)
        exe = casa / 'herdr-pretorian'
        exe.write_bytes(binario)
        exe.chmod(0o700)
        ambiente.update(HOME=str(casa), XDG_DATA_HOME=str(casa / 'data'), XDG_CONFIG_HOME=str(casa / 'config'),
                        XDG_CACHE_HOME=str(casa / 'cache'), PATH='/usr/bin:/bin', LANG='C.UTF-8', TERM='xterm-256color',
                        HERDR_CONFIG_PATH=str(config_path), PRETORIAN_HERDR_CONFIG_DIR=str(casa / 'config/herdr'),
                        PRETORIAN_HERDR_STATE_DIR=str(casa / 'estado'), PRETORIAN_HERDR_SOCKET_DIR=str(casa / 'socket'),
                        SHELL='/bin/sh', USER='pretorian')
        ambiente['PRETORIAN_PROVA_VISUAL'] = marcador
        erro_wm = tempfile.TemporaryFile()
        erro_terminal = tempfile.TemporaryFile()
        etapa = 'fonte'
        ultima_busca = None
        inicio = time.monotonic()
        try:
            subprocess.run(['fc-cache', '-f', str(fonte_path.parent)], env=ambiente, check=True, timeout=20)
            encontrada = subprocess.check_output(['fc-match', '-f', '%{file}', 'JetBrains Mono'], env=ambiente, text=True, timeout=10)
            if Path(encontrada).resolve() != fonte_path.resolve():
                raise ValueError('Fontconfig selecionou outra fonte')
            etapa = 'iniciar_janela'
            wm = subprocess.Popen(['openbox'], env=ambiente, stdout=subprocess.DEVNULL, stderr=erro_wm, start_new_session=True)
            etapa = 'aguardar_gerenciador'
            aguardar_gerenciador(ambiente, wm)
            etapa = 'iniciar_janela'
            comando = ['/usr/bin/xterm'] + args_xterm[1:-2] + [str(exe), '--session', 'pretorian-visual']
            terminal = subprocess.Popen(comando, env=ambiente, cwd=casa / 'trabalho', stdout=subprocess.DEVNULL,
                                        stderr=erro_terminal, start_new_session=True)
            etapa = 'buscar_janela_por_pid'
            janela = ''
            for _ in range(100):
                if terminal.poll() is not None:
                    raise ValueError('Terminal encerrou antes da captura')
                resultado = subprocess.run(['xdotool', 'search', '--onlyvisible', '--pid', str(terminal.pid)], env=ambiente,
                                           capture_output=True, text=True, timeout=3)
                ultima_busca = {'status': resultado.returncode, 'quantidade': len(resultado.stdout.splitlines())}
                if resultado.returncode == 0 and resultado.stdout.strip():
                    janela = resultado.stdout.splitlines()[0]
                    break
                time.sleep(0.1)
            if not janela:
                raise ValueError('Janela não encontrada')
            etapa = 'ativar_janela'
            subprocess.run(['xdotool', 'windowactivate', '--sync', janela], env=ambiente, check=True, timeout=10)
            ativo = False
            for _ in range(100):
                ativo = candidato_no_terminal(exe, terminal.pid)
                if ativo:
                    break
                time.sleep(0.1)
            if not ativo:
                raise ValueError('Herdr candidato não está em execução')
            etapa = 'captura'
            time.sleep(2)
            titulo = subprocess.check_output(['xprop', '-id', janela, 'WM_NAME'], env=ambiente, text=True, timeout=10)
            if '"Pretorian"' not in titulo:
                raise ValueError('Título externo divergente')
            mapas = Path(f'/proc/{terminal.pid}/maps').read_text()
            if str(fonte_path) not in mapas:
                raise ValueError('Uso da fonte pelo xterm não comprovado')
            if terminal.poll() is not None or not candidato_no_terminal(exe, terminal.pid) or digest(exe.read_bytes()) != args.bin_sha:
                raise ValueError('Candidato não está pronto para a captura')
            subprocess.run(['import', '-window', 'root', str(destino / 'tela.png')], env=ambiente, check=True, timeout=10)
            subprocess.run(['import', '-window', janela, str(destino / 'cliente.png')], env=ambiente, check=True, timeout=10)
            if terminal.poll() is not None or not candidato_no_terminal(exe, terminal.pid) or digest(exe.read_bytes()) != args.bin_sha:
                raise ValueError('Candidato não permaneceu ativo durante a captura')
            imagem = (destino / 'tela.png').read_bytes()
            cliente = (destino / 'cliente.png').read_bytes()
            if any(not item.startswith(b'\x89PNG\r\n\x1a\n') or len(item) < 1000 for item in (imagem, cliente)):
                raise ValueError('Captura inválida')
            texto = subprocess.check_output(['tesseract', str(destino / 'cliente.png'), 'stdout'],
                                             env=ambiente, text=True, stderr=subprocess.DEVNULL, timeout=20)
            if 'Pretorian' not in texto:
                raise ValueError('Cabeçalho Pretorian não identificado na captura')
            resultado = {'run': args.run, 'tentativa': args.tentativa, 'artefato': args.artefato, 'commit_fork': args.commit,
                         'sha256_zip': args.zip_sha, 'sha256_executavel': args.bin_sha,
                         'origem_aparencia': origem, 'sha256_tela': digest(imagem), 'sha256_cliente': digest(cliente),
                         'titulo_x11': titulo.strip(),
                         'fonte_resolvida_e_mapeada': True, 'adaptacao': 'Cauda após -e substituída pelo Herdr candidato.',
                         'limite': 'Display virtual; sem lançador, instalação, agente, identidade, login ou homologação.'}
            (destino / 'resultado.json').write_text(json.dumps(resultado, ensure_ascii=False, indent=2) + '\n')
        except Exception as erro:
            diagnostico = {'etapa': etapa, 'erro_tipo': type(erro).__name__,
                           'duracao_segundos': round(time.monotonic() - inicio, 2),
                           'xterm_iniciado': terminal is not None, 'openbox_iniciado': wm is not None,
                           'busca_por_pid': ultima_busca,
                           'xterm_status': terminal.poll() if terminal else None,
                           'openbox_status': wm.poll() if wm else None,
                           'candidato_descendente': candidato_no_terminal(exe, terminal.pid) if terminal else None,
                           'stderr_xterm': stderr_sanitizado(erro_terminal),
                           'stderr_openbox': stderr_sanitizado(erro_wm),
                           'inventario_x11': inventario_janelas(ambiente, terminal.pid if terminal else None)}
            (destino / 'diagnostico.json').write_text(json.dumps(diagnostico, ensure_ascii=False, indent=2) + '\n')
            raise
        finally:
            try:
                subprocess.run([str(exe), '--session', 'pretorian-visual', 'server', 'stop'], env=ambiente,
                               cwd=casa / 'trabalho', stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=5, check=False)
            finally:
                try:
                    parar(terminal)
                finally:
                    try:
                        parar(wm)
                    finally:
                        erro_terminal.close()
                        erro_wm.close()


if __name__ == '__main__':
    main()
