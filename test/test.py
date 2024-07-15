import os


def a():
    file_path = os.path.realpath(os.path.join(__file__, '..', '..', input('File path starting from the lince dir: ')))

    with open(file_path, 'r') as file:
        return print(file.read())


a()
