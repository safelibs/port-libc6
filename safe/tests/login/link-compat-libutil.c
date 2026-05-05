struct termios;
struct winsize;

extern int openpty(
    int *amaster,
    int *aslave,
    char *name,
    const struct termios *termp,
    const struct winsize *winp
);
extern int close(int fd);

int main(void) {
    int master = -1;
    int slave = -1;
    if (openpty(&master, &slave, 0, 0, 0) != 0) {
        return 1;
    }
    close(master);
    close(slave);
    return 0;
}
