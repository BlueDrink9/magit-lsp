"""License boundary and release metadata regression checks."""
import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]


class LicensingTests(unittest.TestCase):
    def test_cargo_component_and_extension_licenses(self):
        cargo = (ROOT / 'Cargo.toml').read_text()
        self.assertRegex(cargo, r'(?m)^license = "GPL-3\.0-only"$')
        self.assertRegex(cargo, r'(?m)^description = "A Git repository interface for editors over the Language Server Protocol"$')
        self.assertRegex(cargo, r'(?m)^repository = "https://github\.com/BlueDrink9/magit-lsp"$')
        self.assertEqual(json.loads((ROOT / 'clients/vscode/package.json').read_text())['license'], 'Apache-2.0')

    def test_apache_notice_copies_and_protocol_boundary(self):
        canonical = (ROOT / 'LICENCES/Apache-2.0').read_bytes()
        for name in ('lua/LICENSE', 'clients/LICENSE', 'clients/vscode/LICENSE', 'docs/LICENSE'):
            self.assertEqual((ROOT / name).read_bytes(), canonical)
        licensing = (ROOT / 'LICENSING.md').read_text()
        for component in ('lua/', 'clients/', 'src/protocol.rs', 'docs/', 'README.md',
                          'TASKS.md', 'AGENTS.md', 'ui-test-plan.md', 'LICENSING.md'):
            self.assertIn(component, licensing)
        self.assertIn('GPL-3.0-only', licensing)

    def test_client_spec_and_readme_acknowledgement(self):
        spec = (ROOT / 'docs/CLIENT_SPEC.md').read_text()
        self.assertIn('Apache-2.0', spec.splitlines()[2])
        readme = (ROOT / 'README.md').read_text()
        self.assertIn('[LICENSING.md]', readme)
        self.assertTrue(readme.rstrip().endswith(
            'Inspired by Magit for Emacs. This project is an independent implementation and is not affiliated with the Magit project.'))
