"""Distribution checks: Calendio must coexist with Calendi."""
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]

class Identity(unittest.TestCase):
    def test_independent_storage(self):
        source = (ROOT / 'crates/core/src/db.rs').read_text()
        self.assertIn('CALENDIO_DB', source)
        self.assertIn('calendio/cal.db', source)
        self.assertNotIn('LYRIC_CAL_DB', source)

    def test_install_does_not_remove_old_app(self):
        source = (ROOT / 'install.sh').read_text()
        self.assertNotIn('rm -f', source)
        # Calendio's own binaries, from a local release build or the CI artifact.
        self.assertIn('from=target/release', source)
        self.assertIn('--name calendio-linux-x86_64', source)
        self.assertIn('"$from/calendio" "$HOME/.local/bin/calendio"', source)
        self.assertIn('calendio-booker.service', source)

    def test_binary_identity(self):
        self.assertIn('name = "calendio"', (ROOT / 'crates/app/Cargo.toml').read_text())
        self.assertTrue((ROOT / 'packaging/calendio.desktop').exists())

if __name__ == '__main__':
    unittest.main()
