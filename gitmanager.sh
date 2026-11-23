#!/bin/sh
# Lance GITManager sur macOS ou Linux, meme sur un ordinateur sans Python.
# Utilise uv s'il est present, sinon Python 3.9 ou plus. S'il n'y a ni l'un
# ni l'autre, installe Python avec le gestionnaire de paquets du systeme, qui
# verifie la signature de ce qu'il telecharge. Le programme installe ensuite
# lui-meme Git et GitHub CLI.
set -u
DOSSIER=$(cd "$(dirname "$0")" && pwd)
SCRIPT="$DOSSIER/gitmanager.py"

trouver_python() {
    for p in python3 python; do
        if command -v "$p" >/dev/null 2>&1 &&
            "$p" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)' 2>/dev/null; then
            PYTHON=$p
            return 0
        fi
    done
    return 1
}

if command -v uv >/dev/null 2>&1; then
    exec uv run "$SCRIPT"
fi
for uv in "$HOME/.local/bin/uv" "$HOME/.cargo/bin/uv"; do
    if [ -x "$uv" ]; then
        exec "$uv" run "$SCRIPT"
    fi
done
if trouver_python; then
    exec "$PYTHON" "$SCRIPT"
fi

echo "GITManager a besoin de Python 3.9 ou plus : installation."
SUDO=""
if [ "$(id -u)" -ne 0 ]; then
    SUDO="sudo"
fi
if [ "$(uname)" = "Darwin" ]; then
    if command -v brew >/dev/null 2>&1; then
        brew install python
    else
        echo "Les outils de developpement d'Apple contiennent Python et Git."
        xcode-select --install
        echo "Relance ce script quand leur installation est terminee."
        exit 1
    fi
elif command -v apt-get >/dev/null 2>&1; then
    $SUDO apt-get update && $SUDO apt-get install -y python3
elif command -v dnf >/dev/null 2>&1; then
    $SUDO dnf install -y python3
elif command -v pacman >/dev/null 2>&1; then
    $SUDO pacman -S --noconfirm --needed python
elif command -v zypper >/dev/null 2>&1; then
    $SUDO zypper --non-interactive install python3
else
    echo "Installe Python 3.9 ou plus depuis https://www.python.org/downloads/ puis relance."
    exit 1
fi

if trouver_python; then
    exec "$PYTHON" "$SCRIPT"
fi
echo "Python n'est pas encore visible : ouvre un nouveau terminal et relance ce script."
exit 1
