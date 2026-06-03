For visibility we need to control what records and transfers can be seen by whom.
That depends on adding

"Quantity works for apples. It is weak for services, promises, knowledge, permissions, access, transportation, or tasks." Thats completely wrong all those things can work in lince, some of them are covered by transfer like transfers are promises before they are excecuted, knowledge can be in body of record, 
Ok, now build me a feature that is enabled in configuration that is the syncing of records head as the
  name of markdown files and the body of records as the content of the files. If that feature is true when
  re restart the app we put it as part of the constants of configuration so that all which may change the
  create, update and delete of records also sync the change with the markdown files (make it so they all
  pass though a function that handles all the current consequences of changing actions). file_sync property
  has an enable bool and a file_sync_path =  "/path" that is a valid path then use that dir as the place
  the files will go, if the dir is not found then put in os_respecive_config_dir/lince/files/, make the
  default be at that place. Whenever lince is started with that config it will have a watcher to that dir
  and changes made to the dir will impact lince while it is on syncing records, when Lince starts it finds
  the difference between it's records and the files and makes Lince's Records the source of truth, setting
  the files on disk to be like Lince's records, with head and body. When you change a Record in Lince the
  function that takes care of changing records also sets up the files to be synced, whenever the watcher
  detects change in files it will send to the Record changing function the order to change Records. If you
  need to put those operations in a queue its ok.