from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts import configurar_terminal


class ConfigurarTerminalTests(unittest.TestCase):
    def test_fragmento_windows_tem_perfil_esquema_e_icone_do_pretorian(self) -> None:
        with tempfile.TemporaryDirectory() as pasta:
            destino = Path(pasta)
            fragmento = configurar_terminal.fragmento_windows(destino)

        perfil = fragmento["profiles"][0]
        esquema = fragmento["schemes"][0]
        self.assertEqual(perfil["name"], "Pretorian")
        self.assertEqual(perfil["tabColor"], "#464c65")
        self.assertEqual(perfil["colorScheme"], esquema["name"])
        self.assertEqual(perfil["icon"], str(destino / "pretorian.ico"))
        self.assertGreaterEqual(perfil["font"]["size"] / 0.75, 14)
        self.assertEqual(esquema["background"], "#ffffff")
        self.assertEqual(esquema["foreground"], "#2f3344")
        self.assertEqual(len([chave for chave in esquema if chave.startswith("bright")]), 8)

    def test_perfil_abre_o_lancador_quando_ele_existe_ao_lado(self) -> None:
        with tempfile.TemporaryDirectory() as pasta:
            destino = Path(pasta)
            self.assertEqual(
                configurar_terminal.comando_do_perfil(destino), destino / "pretorian.exe"
            )
            (destino / "lamarck-pretorian.cmd").write_text("@echo off\n")
            self.assertEqual(
                configurar_terminal.comando_do_perfil(destino),
                destino / "lamarck-pretorian.cmd",
            )

    def test_perfil_macos_define_fundo_texto_e_as_dezesseis_cores(self) -> None:
        cores = configurar_terminal.cores_macos()

        self.assertEqual(cores["BackgroundColor"], "#ffffff")
        self.assertEqual(cores["TextColor"], "#2f3344")
        self.assertEqual(len([chave for chave in cores if chave.startswith("ANSI")]), 16)

    def test_icone_do_perfil_esta_no_repositorio(self) -> None:
        self.assertTrue(configurar_terminal.ICONE.is_file())


if __name__ == "__main__":
    unittest.main()
