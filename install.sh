#!/bin/sh
# Installe gitmanager sur macOS ou Linux : la derniere version publiee sur
# GitHub, verifiee par son empreinte SHA-256, dans ~/.local/bin. La meme
# commande met a jour une version deja installee.
set -eu

depot="SalehKamil/gitmanager"
dossier="${GITMANAGER_DOSSIER:-$HOME/.local/bin}"

case "$(uname -s)" in
  Darwin) os=macos ;;
  Linux) os=linux ;;
  *) echo "Systeme pas pris en charge : $(uname -s). Sous Windows : install.ps1." >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  arm64|aarch64) arch=arm64 ;;
  *) echo "Processeur pas pris en charge : $(uname -m)." >&2; exit 1 ;;
esac
if [ "$os" = linux ] && [ "$arch" = arm64 ]; then
  echo "Linux sur processeur ARM : pas encore d'executable publie." >&2
  exit 1
fi

fichier="gitmanager-$os-$arch"
base="https://github.com/$depot/releases/latest/download"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "  Telechargement de $fichier"
curl -fsSL "$base/$fichier" -o "$tmp/$fichier"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS"

attendue="$(grep " $fichier\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)"
if command -v sha256sum >/dev/null 2>&1; then
  obtenue="$(sha256sum "$tmp/$fichier" | cut -d' ' -f1)"
else
  obtenue="$(shasum -a 256 "$tmp/$fichier" | cut -d' ' -f1)"
fi
if [ -z "$attendue" ] || [ "$attendue" != "$obtenue" ]; then
  echo "  Empreinte SHA-256 differente de celle publiee : installation annulee." >&2
  exit 1
fi
echo "  Empreinte SHA-256 OK : $obtenue"

mkdir -p "$dossier"
chmod +x "$tmp/$fichier"
mv "$tmp/$fichier" "$dossier/gitmanager"
echo "  Installation OK : $dossier/gitmanager"
"$dossier/gitmanager" --version

case ":$PATH:" in
  *":$dossier:"*) ;;
  *)
    case "${SHELL:-}" in
      */zsh) profil="$HOME/.zshrc" ;;
      */bash) profil="$HOME/.bashrc" ;;
      *) profil="$HOME/.profile" ;;
    esac
    ligne="export PATH=\"$dossier:\$PATH\""
    if ! grep -qsF "$ligne" "$profil"; then
      printf '\n# gitmanager\n%s\n' "$ligne" >> "$profil"
    fi
    echo "  Dossier ajoute au PATH dans $profil : ouvrir un nouveau terminal, puis taper gitmanager."
    ;;
esac
