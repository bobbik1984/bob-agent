"""Apply reviewed Bob branding after Android shell generation."""
from pathlib import Path
import shutil
import xml.etree.ElementTree as ET

def stage(app_root):
    generated = app_root / 'src-tauri/gen/android/app/src/main'
    if not generated.is_dir():
        raise RuntimeError('Initialize the Android shell first')
    source = app_root / 'src-tauri/icons/android'
    for item in source.rglob('*'):
        if item.is_file():
            target = generated / 'res' / item.relative_to(source)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(item, target)
            assert target.read_bytes() == item.read_bytes()
    strings = generated / 'res/values/strings.xml'
    tree = ET.parse(strings)
    label = tree.getroot().find("string[@name='app_name']")
    if label is None:
        label = ET.SubElement(tree.getroot(), 'string', {'name':'app_name'})
    label.text = 'Bob'
    tree.write(strings, encoding='utf-8', xml_declaration=True)
    namespace = '{http://schemas.android.com/apk/res/android}'
    application = ET.parse(generated / 'AndroidManifest.xml').getroot().find('application')
    assert application.get(namespace+'icon') == '@mipmap/ic_launcher'
    assert application.get(namespace+'label') == '@string/app_name'
    print('Bob label and launcher resources staged and verified')

if __name__ == '__main__':
    stage(Path(__file__).resolve().parents[1])
