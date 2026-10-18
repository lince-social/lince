idf_component_register(SRCS "main.c" INCLUDE_DIRS "." REQUIRES lince_core lince_workflows bootloader_support PRIV_REQUIRES unity)
if(LINCE_RUN_TESTS)
    target_compile_definitions(${COMPONENT_LIB} PRIVATE LINCE_RUN_TESTS=1)
endif()
