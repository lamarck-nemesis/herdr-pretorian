import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
import zipfile

import prova_visual_linux as prova


class CandidatoVisualTest(unittest.TestCase):
    def test_diagnostico_limita_e_oculta_endereco_caminho_e_credencial(self):
        stderr = io.BytesIO(b'xterm: erro /home/pessoa/privado token=segredo https://exemplo.invalid/a\n')
        resultado = prova.stderr_sanitizado(stderr)
        self.assertNotIn('pessoa', resultado['texto'])
        self.assertNotIn('segredo', resultado['texto'])
        self.assertNotIn('exemplo.invalid', resultado['texto'])
        self.assertIn('xterm: erro', resultado['texto'])
        self.assertFalse(resultado['truncado'])
        self.assertTrue(prova.stderr_sanitizado(io.BytesIO(b'a' * 16385))['truncado'])
        for linha in (b'Authorization: Bearer SEGREDO_SINTETICO\n',
                      b'password="SEGREDO COM ESPACOS"\n'):
            texto = prova.stderr_sanitizado(io.BytesIO(linha))['texto']
            self.assertNotIn('SEGREDO', texto)
            self.assertNotIn('ESPACOS', texto)
            self.assertIn('[removido]', texto)

    def test_inventario_distingue_janela_sem_pid_e_nao_expoe_titulo(self):
        respostas = [subprocess.CompletedProcess([], 0, '42\n', ''),
                     subprocess.CompletedProcess([], 0, '  0x2a "título privado"\n', ''),
                     subprocess.CompletedProcess([], 0, '_NET_WM_PID: not found.\nWM_CLASS(STRING) = "xterm", "XTerm"\nWM_NAME(STRING) = "privado"', ''),
                     subprocess.CompletedProcess([], 0, '  Map State: IsViewable\n', '')]
        with patch.object(prova.subprocess, 'run', side_effect=respostas):
            dados = prova.inventario_janelas({}, 123)
        janela = dados['janelas'][0]
        self.assertIsNone(janela['pid'])
        self.assertTrue(janela['classe_xterm'])
        self.assertEqual(janela['estado'], 'IsViewable')
        self.assertNotIn('privado', json.dumps(dados))

    @unittest.skipUnless(sys.platform.startswith('linux'), 'exige /proc e pidfd Linux')
    def test_falha_limpa_pai_e_filho_de_executavel_distinto_antes_da_pasta(self):
        with tempfile.TemporaryDirectory() as raiz, patch.dict(os.environ, {'RUNNER_TEMP': raiz}):
            marcador = 'teste-visual-exclusivo-' + str(os.getpid())
            processo = None
            casa = None
            try:
                with self.assertRaisesRegex(RuntimeError, 'falha induzida'):
                    with prova.ambiente_temporario(marcador) as casa:
                        env = dict(os.environ, PRETORIAN_PROVA_VISUAL=marcador)
                        processo = subprocess.Popen(['/bin/sh', '-c', '"$2" -c "import time; time.sleep(60)" & echo $! > "$1"; wait',
                                                     'sh', str(casa / 'filho'), sys.executable], env=env)
                        prazo = time.monotonic() + 5
                        while not (casa / 'filho').exists() and time.monotonic() < prazo:
                            time.sleep(0.01)
                        filho = int((casa / 'filho').read_text())
                        while (os.readlink(f'/proc/{processo.pid}/exe') == os.readlink(f'/proc/{filho}/exe')
                               and time.monotonic() < prazo):
                            time.sleep(0.01)
                        self.assertNotEqual(os.readlink(f'/proc/{processo.pid}/exe'), os.readlink(f'/proc/{filho}/exe'))
                        self.assertEqual({processo.pid, filho}, {p for p, _ in prova.processos_marcados(marcador)})
                        raise RuntimeError('falha induzida')
                processo.wait(timeout=5)
                self.assertEqual(prova.processos_marcados(marcador), [])
                self.assertFalse(casa.exists())
            finally:
                if processo and processo.poll() is None:
                    processo.kill()
                    processo.wait()

    def test_rejeita_adulteracao_e_preserva_binario_candidato(self):
        with tempfile.TemporaryDirectory() as d:
            pasta = Path(d)
            commit = 'a' * 40
            binario = b'executavel sintetico nao executado'
            nome = f'pretorian-fork-linux-x64-{commit[:12]}.zip'
            arquivo = pasta / nome
            with zipfile.ZipFile(arquivo, 'w') as z:
                z.writestr('herdr-pretorian', binario)
                z.writestr('licencas/aviso', b'aviso')
            bin_sha = hashlib.sha256(binario).hexdigest()
            zip_sha = hashlib.sha256(arquivo.read_bytes()).hexdigest()
            registro = {'estado': 'candidato', 'repositorio': 'lamarck-nemesis/herdr-pretorian',
                        'versao': 'candidato-' + commit[:12], 'fonte_commit': commit,
                        'artefatos': {'linux-x64': {'arquivo': nome, 'sha256': zip_sha,
                            'arquivos': {'herdr-pretorian': bin_sha, 'licencas/aviso': hashlib.sha256(b'aviso').hexdigest()}}}}
            (pasta / 'candidato.json').write_text(json.dumps(registro))
            self.assertEqual(prova.ler_candidato(pasta, commit, zip_sha, bin_sha), binario)
            with self.assertRaises(ValueError):
                prova.ler_candidato(pasta, 'b' * 40, zip_sha, bin_sha)
            arquivo.write_bytes(arquivo.read_bytes() + b'adulterado')
            with self.assertRaises(ValueError):
                prova.ler_candidato(pasta, commit, zip_sha, bin_sha)


if __name__ == '__main__':
    unittest.main()
