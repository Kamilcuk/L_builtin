.PHONY: all clean

all:
	mkdir -p build
	cd build
	cmake .. -DL_DISPATCHER_SO_FILES="${L_DISPATCHER_SO_FILES}"
	$(MAKE)

clean:
	rm -rf build
