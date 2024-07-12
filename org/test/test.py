# options_header = ['[S] Save.', '[E] Exit.'] 
# operation_options = ['[C] Create.','[R] Read.', '[U] Update.', '[D] Delete.']
# table_options = ['[1] Cadastro']

options_header = ['[S] Save.', '[E] Exit.'] 
operation_options = ['[C] Create.','[R] Read.', '[U] Update.', '[D] Delete3333333331.']
table_options = ['[1] Cadastro']

# Create a list of lengths for each list
# lengths = [len(item) for item in options_header + operation_options + table_options]

print(max([len(item) for item in options_header + operation_options + table_options]))
