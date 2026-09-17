package main
import "net/http"
func health(w http.ResponseWriter, r *http.Request) {}
func main() { http.HandleFunc("/health", health); http.ListenAndServe(":8080", nil) }
