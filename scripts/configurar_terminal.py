"""Configura o terminal do sistema com o perfil Pretorian depois de `just instalar`.

Windows: fragmento do Windows Terminal (perfil, esquema de cores, ícone) e atalho no menu Iniciar.
macOS: perfil "Pretorian" do Terminal, importado e definido como padrão.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
ICONE = REPO_ROOT / "assets" / "pretorian" / "pretorian.ico"
ICONE_PNG = REPO_ROOT / "assets" / "pretorian" / "pretorian.png"

NOME = "Pretorian"
# Marca mostrada na barra de título da janela.
MARCA = "LSA • prεtοrιαn"
# GUID fixo: o Windows Terminal reconhece o mesmo perfil a cada reinstalação.
GUID_PERFIL = "{6f1c2a5e-3b7d-5c41-9e2a-70726574726e}"
MAUVE = "#464c65"
FUNDO = "#ffffff"
TEXTO = "#2f3344"
SELECAO = "#e0e4eb"
TAMANHO_FONTE_PX = 14

# Cores ANSI legíveis sobre fundo claro (Catppuccin Latte, base do tema Pretorian).
ANSI = {
    "black": "#5c5f77",
    "red": "#d20f39",
    "green": "#40a02b",
    "yellow": "#df8e1d",
    "blue": "#1e66f5",
    "purple": "#ea76cb",
    "cyan": "#179299",
    "white": "#acb0be",
    "brightBlack": "#6c6f85",
    "brightRed": "#d20f39",
    "brightGreen": "#40a02b",
    "brightYellow": "#df8e1d",
    "brightBlue": "#1e66f5",
    "brightPurple": "#ea76cb",
    "brightCyan": "#179299",
    "brightWhite": "#bcc0cc",
}


def comando_do_perfil(destino: Path) -> Path:
    """O lançador `lamarck-pretorian` mora na mesma pasta quando o Pretorian CLI está instalado."""
    lancador = destino / "lamarck-pretorian.cmd"
    return lancador if lancador.exists() else destino / "pretorian.exe"


def fragmento_windows(destino: Path) -> dict:
    return {
        "profiles": [
            {
                "guid": GUID_PERFIL,
                "name": NOME,
                "commandline": f'"{comando_do_perfil(destino)}"',
                "icon": str(destino / ICONE.name),
                "colorScheme": NOME,
                "tabColor": MAUVE,
                "tabTitle": MARCA,
                "cursorColor": MAUVE,
                # O Windows Terminal mede a fonte em pontos: 14 px = 10,5 pt.
                "font": {"face": "Cascadia Mono", "size": TAMANHO_FONTE_PX * 0.75},
                "padding": "12",
            }
        ],
        "schemes": [
            {
                "name": NOME,
                "background": FUNDO,
                "foreground": TEXTO,
                "cursorColor": MAUVE,
                "selectionBackground": SELECAO,
                **ANSI,
            }
        ],
    }


def configurar_windows(destino: Path, local_app_data: Path, app_data: Path) -> list[Path]:
    destino.mkdir(parents=True, exist_ok=True)
    icone = destino / ICONE.name
    shutil.copyfile(ICONE, icone)

    fragmento = local_app_data / "Microsoft" / "Windows Terminal" / "Fragments" / NOME / "pretorian.json"
    fragmento.parent.mkdir(parents=True, exist_ok=True)
    fragmento.write_text(json.dumps(fragmento_windows(destino), indent=2) + "\n", encoding="utf-8")

    atalho = app_data / "Microsoft" / "Windows" / "Start Menu" / "Programs" / f"{NOME}.lnk"
    atalho.parent.mkdir(parents=True, exist_ok=True)
    criar_atalho = (
        "$a = (New-Object -ComObject WScript.Shell).CreateShortcut($env:PRETORIAN_ATALHO);"
        "$a.TargetPath = 'wt.exe';"
        f"$a.Arguments = '-p \"{NOME}\"';"
        "$a.IconLocation = $env:PRETORIAN_ICONE;"
        "$a.Save()"
    )
    subprocess.run(
        ["powershell.exe", "-NoProfile", "-Command", criar_atalho],
        check=True,
        env={**os.environ, "PRETORIAN_ATALHO": str(atalho), "PRETORIAN_ICONE": str(icone)},
    )
    return [icone, fragmento, atalho]


def _rgb(cor: str) -> tuple[float, float, float]:
    return tuple(int(cor[i : i + 2], 16) / 255 for i in (1, 3, 5))


def cores_macos() -> dict[str, str]:
    nomes = {
        "black": "Black",
        "red": "Red",
        "green": "Green",
        "yellow": "Yellow",
        "blue": "Blue",
        "purple": "Magenta",
        "cyan": "Cyan",
        "white": "White",
    }
    cores = {
        "BackgroundColor": FUNDO,
        "TextColor": TEXTO,
        "TextBoldColor": TEXTO,
        "CursorColor": MAUVE,
        "SelectionColor": SELECAO,
    }
    for chave, nome in nomes.items():
        cores[f"ANSI{nome}Color"] = ANSI[chave]
        cores[f"ANSIBright{nome}Color"] = ANSI["bright" + chave[0].upper() + chave[1:]]
    return cores


TITULO_OCULTO_MACOS = [
    "ShowActiveProcessInTitle",
    "ShowActiveProcessArgumentsInTitle",
    "ShowActiveProcessInTabTitle",
    "ShowActiveProcessArgumentsInTabTitle",
    "ShowCommandKeyInTitle",
    "ShowDimensionsInTitle",
    "ShowRepresentedURLPathInTitle",
    "ShowShellCommandInTitle",
    "ShowTTYNameInTitle",
    "ShowWindowSettingsNameInTitle",
]

# O Terminal guarda cores e fonte como objetos arquivados; só o AppKit os produz.
GERADOR_PERFIL_MACOS = r"""
ObjC.import('AppKit');
function run(argv) {
  const entrada = JSON.parse(argv[0]);
  const arquivar = (objeto) => $.NSKeyedArchiver.archivedDataWithRootObject(objeto);
  const perfil = $.NSMutableDictionary.alloc.init;
  perfil.setObjectForKey($(entrada.nome), 'name');
  perfil.setObjectForKey($('Window Settings'), 'type');
  perfil.setObjectForKey($.NSNumber.numberWithDouble(2.07), 'ProfileCurrentVersion');
  perfil.setObjectForKey($.NSNumber.numberWithBool(true), 'UseBrightBold');
  // A barra de título mostra só o ícone, a marca e o título que o Pretorian define.
  for (const chave of entrada.titulo_oculto) {
    perfil.setObjectForKey($.NSNumber.numberWithBool(false), chave);
  }
  perfil.setObjectForKey($.NSNumber.numberWithBool(true), 'ShowRepresentedURLInTitle');
  for (const chave of Object.keys(entrada.cores)) {
    const [r, g, b] = entrada.cores[chave];
    perfil.setObjectForKey(arquivar($.NSColor.colorWithSRGBRedGreenBlueAlpha(r, g, b, 1)), chave);
  }
  const fonte = $.NSFont.fontWithNameSize('SFMono-Regular', entrada.tamanho);
  perfil.setObjectForKey(
    arquivar(fonte.isNil() ? $.NSFont.userFixedPitchFontOfSize(entrada.tamanho) : fonte),
    'Font'
  );
  if (!perfil.writeToFileAtomically(entrada.destino, true)) {
    throw new Error('não foi possível gravar ' + entrada.destino);
  }
  // O Terminal mostra na barra de título o ícone do arquivo que a sessão representa (OSC 6).
  $.NSFileManager.defaultManager.createFileAtPathContentsAttributes(entrada.marca, $.NSData.data, $());
  const icone = $.NSImage.alloc.initWithContentsOfFile(entrada.icone);
  if (!$.NSWorkspace.sharedWorkspace.setIconForFileOptions(icone, entrada.marca, 0)) {
    throw new Error('não foi possível aplicar o ícone em ' + entrada.marca);
  }
}
"""


def configurar_macos(pasta: Path, padrao: bool) -> list[Path]:
    pasta.mkdir(parents=True, exist_ok=True)
    perfil = pasta / f"{NOME}.terminal"
    entrada = {
        "nome": NOME,
        "destino": str(perfil),
        "tamanho": TAMANHO_FONTE_PX,
        "marca": str(pasta / MARCA),
        "icone": str(ICONE_PNG),
        "titulo_oculto": TITULO_OCULTO_MACOS,
        "cores": {chave: _rgb(cor) for chave, cor in cores_macos().items()},
    }
    subprocess.run(
        ["osascript", "-l", "JavaScript", "-e", GERADOR_PERFIL_MACOS, json.dumps(entrada)],
        check=True,
    )
    # Reimportar sem apagar cria "Pretorian 1" em vez de atualizar o perfil.
    subprocess.run(
        [
            "osascript",
            "-e",
            f'tell application "Terminal" to delete (every settings set whose name starts with "{NOME}")',
        ],
        check=False,
        capture_output=True,
    )
    # Abrir o arquivo é a forma suportada de importar um perfil no Terminal.
    subprocess.run(["open", "-g", "-a", "Terminal", str(perfil)], check=True)
    if padrao:
        subprocess.run(
            [
                "osascript",
                "-e",
                f'tell application "Terminal" to set default settings to settings set "{NOME}"',
                "-e",
                f'tell application "Terminal" to set startup settings to settings set "{NOME}"',
            ],
            check=True,
        )
    return [perfil, pasta / MARCA]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--destino", type=Path, help="pasta onde o Pretorian foi instalado (Windows)")
    parser.add_argument(
        "--sem-padrao",
        action="store_true",
        help="macOS: importa o perfil sem torná-lo o padrão do Terminal",
    )
    args = parser.parse_args(argv)

    if sys.platform == "win32":
        destino = args.destino or Path(os.environ["LOCALAPPDATA"]) / "Programs" / "pretorian"
        criados = configurar_windows(
            destino, Path(os.environ["LOCALAPPDATA"]), Path(os.environ["APPDATA"])
        )
    elif sys.platform == "darwin":
        criados = configurar_macos(
            Path.home() / "Library" / "Application Support" / "pretorian-terminal", not args.sem_padrao
        )
    else:
        print("terminal não configurado: o perfil Pretorian existe para Windows e macOS")
        return 0
    for caminho in criados:
        print(f"terminal configurado: {caminho}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
