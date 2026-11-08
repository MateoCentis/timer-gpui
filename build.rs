fn main() {
    println!("cargo:rerun-if-changed=assets/temporizador.ico");

    // GPUI y la bandeja cargan el ícono desde el recurso #1 del ejecutable, que es el
    // que `set_icon` incrusta. Así lo usan también Explorer y la barra de tareas.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/temporizador.ico")
            .set("FileDescription", "Temporizador")
            .set("ProductName", "Temporizador");
        res.compile().expect("no se pudo incrustar el ícono");
    }
}
