def a(parameter):
    if ('c' or 'C') in parameter:
        return print(False)
    return print(True)


print('--')
a("b1")
