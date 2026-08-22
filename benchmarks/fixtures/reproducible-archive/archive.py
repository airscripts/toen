from io import BytesIO
from zipfile import ZIP_DEFLATED, ZipFile


def build_archive(files):
    output = BytesIO()

    with ZipFile(output, "w", ZIP_DEFLATED) as archive:
        for name, contents in sorted(files.items()):
            archive.writestr(name, contents)

    return output.getvalue()
