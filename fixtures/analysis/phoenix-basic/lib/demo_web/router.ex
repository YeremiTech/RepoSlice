defmodule DemoWeb.Router do
  use DemoWeb, :router
  scope "/api" do
    get "/health", HealthController, :show
  end
end
